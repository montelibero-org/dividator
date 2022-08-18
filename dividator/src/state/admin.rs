use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Alias for hex encoded k1 nonce
pub type K1 = String;
/// Alias for hex encoded compressed (33 bytes) secp256k1 pubkey
pub type PublicKey = String;
/// Alias for der encoded secp256k1 signature
pub type Signature = String;

/// Admin info that we keep in memory
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdminInfo {
    /// Public key that was used in initial linking
    pub key: PublicKey,
    /// Time of creation
    pub created_at: NaiveDateTime,
}

/// Action to add new admin to the system
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddAdmin {
    /// Public key of admin
    pub key: PublicKey,
    /// 32 byte hex encoded nonce
    pub k1: K1,
    /// Der encoded signature of the k1
    pub signature: Signature,
    /// Time of the event
    pub timestamp: NaiveDateTime,
}

impl From<AddAdmin> for AdminInfo {
    fn from(v: AddAdmin) -> Self {
        AdminInfo {
            key: v.key,
            created_at: v.timestamp,
        }
    }
}
