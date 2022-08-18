use super::types::Permission;
use super::{
    auth_handler, generate_auth_lnurl, generate_k1, generate_qrcode, AuthResponse, LnAuthAction,
    AUTH_COOKIE,
};
use crate::api::types::*;
use chrono::prelude::*;
use dividator::state::admin::AddAdmin;
use dividator::state::SystemUpdate;
use dividator::state::K1;
use log::*;
use rocket::fs::NamedFile;
use rocket::get;
use rocket::http::Status;
use rocket::http::{Cookie, CookieJar};
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::uri;
use rocket::State;
use rocket_dyn_templates::Template;
use rocket_okapi::openapi;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;

/// <LNURL_hostname_and_path>?<LNURL_existing_query_parameters>&sig=<hex(sign(hexToBytes(k1), linkingPrivKey))>&key=<hex(linkingKey)>
#[openapi(tag = "auth")]
#[get("/?tag=login&<k1>&<action>&<sig>&<key>")]
pub async fn index_handler(
    cache_mutex: &State<AuthCache>,
    db_mutex: &State<DataBase>,
    k1_sender: &State<broadcast::Sender<K1>>,
    action: LnAuthAction,
    k1: String,
    sig: String,
    key: String,
) -> Json<AuthResponse> {
    match action {
        LnAuthAction::Login => handler_signin(cache_mutex, db_mutex, k1_sender, k1, sig, key).await,
        LnAuthAction::Register => {
            handler_register(cache_mutex, db_mutex, k1_sender, k1, sig, key).await
        }
        _ => Json(AuthResponse::Error {
            reason: "Unexpected action tag".to_owned(),
        }),
    }
}

async fn handler_signin(
    cache_mutex: &State<AuthCache>,
    db_mutex: &State<DataBase>,
    k1_sender: &State<broadcast::Sender<K1>>,
    k1: String,
    sig: String,
    key: String,
) -> Json<AuthResponse> {
    let mut cache = cache_mutex.lock().await;
    let db = db_mutex.lock().await;
    let res = auth_handler(&mut cache, &k1, &sig, &key).await;
    match &res {
        AuthResponse::Ok => {
            if let Some(akey) = db.get().await.admin_key() {
                if akey == key {
                    info!("Admin logged in!");
                    cache.upsert_session(&k1, &key, &vec![Permission::Admin]);
                    if let Err(_) = k1_sender.send(k1.clone()) {
                        error!("Failed to notify k1 listeners!");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        if let Err(_) = k1_sender.send(k1) {
                            error!("Failed to notify k1 listeners x2!");
                        }
                    }
                    Json(AuthResponse::Ok)
                } else {
                    Json(AuthResponse::Error {
                        reason: "User is not known".to_owned(),
                    })
                }
            } else {
                Json(AuthResponse::Error {
                    reason: "Admin is not defined".to_owned(),
                })
            }
        }
        AuthResponse::Error { reason } => {
            warn!("User failed to logg in: {}", reason);
            Json(res)
        }
    }
}

async fn handler_register(
    cache_mutex: &State<AuthCache>,
    db_mutex: &State<DataBase>,
    k1_sender: &State<broadcast::Sender<K1>>,
    k1: String,
    sig: String,
    key: String,
) -> Json<AuthResponse> {
    let mut db = db_mutex.lock().await;
    if db.get().await.has_admin() {
        Json(AuthResponse::Error {
            reason: "Admin already exists".to_owned(),
        })
    } else {
        let mut cache = cache_mutex.lock().await;
        let res = auth_handler(&mut cache, &k1, &sig, &key).await;
        match &res {
            AuthResponse::Ok => {
                info!("Admin passes check to register!");
                let dbres = db
                    .update(SystemUpdate::AddAdmin(AddAdmin {
                        key: key.clone(),
                        k1: k1.clone(),
                        signature: sig,
                        timestamp: Utc::now().naive_utc(),
                    }))
                    .await;
                match dbres {
                    Ok(_) => {
                        info!("Admin finalized");
                        cache.upsert_session(&k1, &key, &vec![Permission::Admin]);
                        if let Err(_) = k1_sender.send(k1.clone()) {
                            error!("Failed to notify k1 listeners!");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            if let Err(_) = k1_sender.send(k1) {
                                error!("Failed to notify k1 listeners x2!");
                            }
                        }
                        Json(res)
                    }
                    Err(e) => {
                        error!("Failed to update admin info: {}", e);
                        Json(AuthResponse::Error {
                            reason: "Internal error, failed to setup admin".to_owned(),
                        })
                    }
                }
            }
            AuthResponse::Error { reason } => {
                warn!("Admin failed to register: {}", reason);
                Json(res)
            }
        }
    }
}

#[openapi(skip)]
#[get("/init")]
pub async fn init(
    db_mutex: &State<DataBase>,
    cache_mutex: &State<AuthCache>,
    domain: &State<String>,
) -> Result<Template, Result<Redirect, Status>> {
    let db = db_mutex.lock().await;
    if db.get().await.has_admin() {
        Err(Ok(Redirect::to(uri!("/"))))
    } else {
        let k1 = generate_k1();
        let lnurl = generate_auth_lnurl(domain, &k1, LnAuthAction::Register).map_err(Err)?;
        let mut cache = cache_mutex.lock().await;
        cache.add(&k1);
        let context = HashMap::from([
            ("title", "Bind admin wallet"),
            ("parent", "base"),
            ("k1", &k1),
            ("lnurl", &lnurl),
        ]);
        Ok(Template::render("init", context))
    }
}

#[openapi(tag = "events")]
#[get("/signin/poll?<k1>")]
pub async fn signin_poll(
    cookies: &CookieJar<'_>,
    k1_sender: &State<broadcast::Sender<K1>>,
    k1: String,
) -> Redirect {
    trace!("Awaiting signin finalization");
    let polling_timeout = Duration::from_secs(300);
    let mut k1_receiver = k1_sender.subscribe();
    loop {
        match timeout(polling_timeout, k1_receiver.recv()).await {
            Ok(Ok(k1_received)) if k1_received == k1 => {
                trace!("Redirect client to index");
                cookies.add_private(Cookie::new(AUTH_COOKIE, k1));
                return Redirect::to(uri!("/"));
            }
            Ok(Ok(_)) => {
                trace!("Not ours k1");
            }
            Ok(Err(_)) => {
                error!("We lagged behind high frequently updated k1!");
            }
            Err(_) => {
                trace!("No new events but releasing long poll");
                return Redirect::to(uri!(signin));
            }
        }
    }
}

#[openapi(skip)]
#[get("/signin")]
pub async fn signin(
    cache_mutex: &State<AuthCache>,
    cookies: &CookieJar<'_>,
    domain: &State<String>,
) -> Result<Template, Result<Redirect, Status>> {
    if let Some(_) = cookies.get_private(AUTH_COOKIE) {
        Err(Ok(Redirect::to(uri!("/"))))
    } else {
        let k1 = generate_k1();
        let lnurl = generate_auth_lnurl(domain, &k1, LnAuthAction::Login).map_err(Err)?;
        let mut cache = cache_mutex.lock().await;
        cache.add(&k1);
        let context = HashMap::from([
            ("title", "Sign in"),
            ("parent", "base"),
            ("k1", &k1),
            ("lnurl", &lnurl),
        ]);
        Ok(Template::render("signin", context))
    }
}

#[openapi(skip)]
#[get("/signout")]
pub async fn signout(cookies: &CookieJar<'_>) -> Redirect {
    if let Some(v) = cookies.get_private(AUTH_COOKIE) {
        cookies.remove_private(v);
    }
    Redirect::to(uri!(signin))
}

#[openapi(skip)]
#[get("/qrcode/signin?<k1>")]
pub async fn get_qrcode_endpoint(
    domain: &State<String>,
    cache_mutex: &State<AuthCache>,
    k1: String,
) -> Result<NamedFile, Status> {
    let color = [68, 32, 77];
    let mut cache = cache_mutex.lock().await;
    if cache.pick(&k1) {
        generate_qrcode(
            &mut cache,
            &domain,
            color,
            (300, 300),
            LnAuthAction::Login,
            &k1,
        )
        .await
    } else {
        error!("Unknown k1 value from client!");
        Err(Status::BadRequest)
    }
}

#[openapi(skip)]
#[get("/qrcode/admin?<k1>")]
pub async fn get_qrcode_admin_endpoint(
    domain: &State<String>,
    cache_mutex: &State<AuthCache>,
    k1: String,
) -> Result<NamedFile, Status> {
    let color = [68, 32, 77];
    let mut cache = cache_mutex.lock().await;
    if cache.pick(&k1) {
        generate_qrcode(
            &mut cache,
            &domain,
            color,
            (300, 300),
            LnAuthAction::Register,
            &k1,
        )
        .await
    } else {
        error!("Unknown k1 value from client!");
        Err(Status::BadRequest)
    }
}
