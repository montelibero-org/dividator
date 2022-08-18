use super::types::*;
use chrono::prelude::*;
use chrono::Duration;
use dividator::state::{PublicKey, K1};
use std::collections::{HashMap, HashSet};

/// Cache for used k1 nonces that are signed by wallet.
///
/// From LNURL specs:
/// ```
/// LN SERVICE must make sure that unexpected k1s are not
/// accepted: it is strongly advised for LN SERVICE to
/// have a cache of unused k1s, only proceed with
/// verification of k1s present in that cache and remove
/// used k1s on successful auth attempts.
/// ```
pub struct Cache {
    /// Amount of time we expect to wallet to login
    pub login_timeout: Duration,
    /// Amount of time session persists without activity
    pub session_timeout: Duration,
    /// Cached K1 keys that we allow to login
    pub keys: HashMap<K1, NaiveDateTime>,
    /// Memorized sessions of logged users
    pub sessions: HashMap<K1, SessionInfo>,
}

/// Session runtime information
pub struct SessionInfo {
    /// Wallet linking key
    pub key: PublicKey,
    /// When the session will die
    pub timeout: NaiveDateTime,
    /// Permissions of the session
    pub permissions: HashSet<Permission>,
}

impl SessionInfo {
    /// Return `true` if the session has all the required permissions
    pub fn check_permissions(&self, perms: &[Permission]) -> bool {
        for p in perms {
            if !self.permissions.contains(p) {
                return false;
            }
        }
        return true;
    }
}

impl Cache {
    pub fn new(login_timeout: Duration, session_timeout: Duration) -> Self {
        Cache {
            login_timeout,
            session_timeout,
            keys: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    /// Add given k1 secret to internal storage with timeout.
    /// Also removes outdated keys from the map.
    pub fn add(&mut self, k1: &str) {
        self.cleanup();
        self.keys
            .insert(k1.to_owned(), Utc::now().naive_utc() + self.login_timeout);
    }

    /// Check if the key is located in the cache
    /// if so return `true` and remove it from the cache.
    /// If there is no such key, returns `false`.
    pub fn pick(&mut self, k1: &str) -> bool {
        if self.keys.contains_key(k1) {
            self.keys.remove(k1);
            true
        } else {
            false
        }
    }

    /// Check if we know that k1 key as active session
    pub fn has_session(&mut self, k1: &str) -> Option<&SessionInfo> {
        self.cleanup();
        self.sessions.get(k1)
    }

    /// Create session or update existing one
    pub fn upsert_session(&mut self, k1: &str, pub_key: &str, permissions: &[Permission]) {
        self.cleanup();
        let now = Utc::now().naive_utc();
        match self.sessions.get_mut(k1) {
            Some(r) => r.timeout = now + self.session_timeout,
            None => {
                self.sessions.insert(
                    k1.to_owned(),
                    SessionInfo {
                        key: pub_key.to_owned(),
                        timeout: now + self.session_timeout,
                        permissions: permissions.iter().cloned().collect(),
                    },
                );
            }
        }
    }

    // pub fn touch_session(&mut self, k1: &str)
    /// Cleanup outdated keys from the cache.
    pub fn cleanup(&mut self) {
        let now = Utc::now().naive_utc();
        self.keys.retain(|_, t| *t >= now);
        self.sessions.retain(|_, v| v.timeout >= now);
    }
}

impl Default for Cache {
    /// 5 minutes default timeout for login and 30 minutes for session
    fn default() -> Self {
        Cache::new(Duration::minutes(5), Duration::minutes(30))
    }
}
