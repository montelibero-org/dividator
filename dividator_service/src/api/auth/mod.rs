pub mod cache;
pub mod routes;
pub mod types;

use super::types::*;
use bech32::{ToBase32, Variant};
use cache::Cache;
use futures::future::Future;
use image::Rgb;
use log::*;
use qrcode::QrCode;
use rand::distributions::Uniform;
use rand::Rng;
use rocket::fs::NamedFile;
use rocket::http::CookieJar;
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::uri;
use rocket_okapi::JsonSchema;
use secp256k1::{Message, Secp256k1};
use serde::Serialize;
use std::str::FromStr;
use tempfile::tempdir;
use thiserror::Error;
pub use types::{LnAuthAction, Permission};

/// Generate 32 bytes and encode them as hex string for LNURL
pub fn generate_k1() -> String {
    let k1: Vec<u8> = rand::thread_rng()
        .sample_iter(Uniform::new_inclusive(0, u8::MAX))
        .take(32)
        .collect();
    hex::encode(&k1)
}

/// Generate a lnurl for handler with random k1 and login tag
pub fn generate_auth_lnurl(domain: &str, k1: &str, action: LnAuthAction) -> Result<String, Status> {
    let handler = format!("{}?tag=login&k1={}&action={}", domain, k1, action);
    bech32::encode("lnurl", handler.as_bytes().to_base32(), Variant::Bech32).map_err(|e| {
        error!("Failed to encode bech32 lnurl: {}", e);
        Status::InternalServerError
    })
}

/// Generate QR code for LNURL auth to temp folder and return it as rocket responser.
///
/// # Arguments
///
/// * `domain` - which domain to use in LNURL. Including https. Example: `https://domain.sample`
/// * `color` - RGB for dark part of the QR code
/// * `dims` - size of resulted image, width and height.
pub async fn generate_qrcode(
    cache: &mut Cache,
    domain: &str,
    color: [u8; 3],
    dims: (u32, u32),
    action: LnAuthAction,
    k1: &str,
) -> Result<NamedFile, Status> {
    // Encode some data into bits.
    let bech_handler = generate_auth_lnurl(domain, k1, action)?;

    let code = QrCode::new(bech_handler).map_err(|e| {
        error!("Failed to create qrcode: {}", e);
        Status::InternalServerError
    })?;

    // Render the bits into an image.
    let image = code
        .render::<Rgb<u8>>()
        .dark_color(Rgb(color))
        .min_dimensions(dims.0, dims.1)
        .build();

    // Save to temporary folder
    let dir = tempdir().map_err(|e| {
        error!("Failed to create temp dir for qrcode: {}", e);
        Status::InternalServerError
    })?;
    let qrcode_path = dir.path().join("qrcode.png");
    image.save(&qrcode_path).unwrap();
    let opened = NamedFile::open(qrcode_path).await.map_err(|e| {
        error!("Failed to open generated qrcode: {}", e);
        Status::InternalServerError
    })?;

    // Store the k1 key in the cache
    cache.add(&k1);
    Ok(opened)
}

/// Response for auth handler
#[derive(Serialize, JsonSchema)]
#[serde(tag = "status")]
pub enum AuthResponse {
    #[serde(rename = "OK")]
    Ok,
    #[serde(rename = "ERROR")]
    Error { reason: String },
}

/// Validates signagute, that k1 is known and returns `AuthResponse::Ok` if all checks
/// are passed.
pub async fn auth_handler(cache: &mut Cache, k1: &str, sig: &str, key: &str) -> AuthResponse {
    if cache.pick(&k1) {
        match check_signature(&k1, &sig, &key) {
            Ok(_) => AuthResponse::Ok,
            Err(e) => AuthResponse::Error {
                reason: format!("{}", e),
            },
        }
    } else {
        AuthResponse::Error {
            reason: "Outdated or unknown k1. Please, scan QR code once more time.".to_owned(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SigError {
    #[error("k1 is not hex encoded: {0}")]
    HexK1(hex::FromHexError),
    #[error("Failed to decode hex encoded k1: {0}")]
    InvalidK1(secp256k1::Error),
    #[error("Failed to decode hex public key: {0}")]
    KeyDecode(secp256k1::Error),
    #[error("Failed to decode DER signature: {0}")]
    SignatureDecode(secp256k1::Error),
    #[error("Signature check failed: {0}")]
    SignatureVerify(secp256k1::Error),
}

/// Check that signature of k1 and user pubkey is valid and return `true` in that case.
/// Returns `false` also if the key or signature is in invalid encoding.
pub fn check_signature(k1: &str, sig: &str, key: &str) -> Result<(), SigError> {
    let k1_bytes = hex::decode(k1.as_bytes()).map_err(SigError::HexK1)?;
    let message = Message::from_slice(&k1_bytes).map_err(SigError::InvalidK1)?;
    let secp = Secp256k1::new();
    let public_key = secp256k1::PublicKey::from_str(key).map_err(SigError::KeyDecode)?;
    let signature =
        secp256k1::ecdsa::Signature::from_str(sig).map_err(SigError::SignatureDecode)?;
    secp.verify_ecdsa(&message, &signature, &public_key)
        .map_err(SigError::SignatureVerify)?;
    Ok(())
}

/// Cookie name that contains session id
pub const AUTH_COOKIE: &str = "session";

/// Helper that allows to wrap any endpoint and gurantee
/// that user passed the authentification with given permissions.
pub async fn guard_auth<Fut, T>(
    db_mutex: &DataBase,
    cookies: &CookieJar<'_>,
    cache_mutex: AuthCache,
    permissions: &[Permission],
    body: Fut,
) -> Result<T, Redirect>
where
    Fut: Future<Output = T>,
{
    let has_admin = {
        let db = db_mutex.lock().await;
        let state = db.get().await;
        state.has_admin()
    };
    if has_admin {
        if let Some(k1) = cookies.get_private(AUTH_COOKIE) {
            let mut cache = cache_mutex.lock().await;
            if let Some(session) = cache.has_session(k1.value()) {
                if session.check_permissions(permissions) {
                    let pubkey = session.key.clone();
                    cache.upsert_session(k1.value(), &pubkey, permissions);
                    Ok(body.await)
                } else {
                    warn!(
                        "User doesn' have required permissions: {:?}, user perms: {:?}",
                        permissions, session.permissions
                    );
                    Err(Redirect::to(uri!("/")))
                }
            } else {
                warn!("User session expired, k1: {}", k1);
                cookies.remove_private(k1);
                Err(Redirect::to(uri!(routes::signin)))
            }
        } else {
            info!("User is not logged in, redirect to signin");
            Err(Redirect::to(uri!(routes::signin)))
        }
    } else {
        info!("System is not fully initialized, redirect to init");
        Err(Redirect::to(uri!(routes::init)))
    }
}
