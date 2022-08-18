pub mod admin;

use admin::AddAdmin;
pub use admin::{AdminInfo, PublicKey, K1};
use append_db::State;
use append_db_postgres::HasUpdateTag;
use append_db_postgres::VersionedState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, VersionedState)]
pub struct SystemState {
    pub admin: Option<AdminInfo>,
}

impl SystemState {
    pub fn new() -> Self {
        SystemState {
            admin: None,
        }
    }

    /// If there any admin key linked returns `true`
    pub fn has_admin(&self) -> bool {
        self.admin.is_some()
    }

    pub fn admin_key(&self) -> Option<PublicKey> {
        self.admin.as_ref().map(|v| v.key.clone())
    }
}

impl Default for SystemState {
    fn default() -> Self {
        SystemState::new()
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("We already has admin account linked")]
    AdminRegistered,
}

/// All updates of database goes through that updates
#[derive(Clone, Debug, PartialEq, HasUpdateTag)]
pub enum SystemUpdate {
    /// Link wallet to admin account
    AddAdmin(AddAdmin),
    /// Cleanup admin information. Can be done only via CLI.
    /// Empty tuple is required to make deriving happy.
    CleanAdmin(()),
}

impl State for SystemState {
    type Update = SystemUpdate;
    type Err = Error;

    fn update(&mut self, upd: SystemUpdate) -> Result<(), Self::Err> {
        match upd {
            SystemUpdate::AddAdmin(_) if self.has_admin() => {
                return Err(Error::AdminRegistered);
            }
            SystemUpdate::AddAdmin(v) => {
                self.admin = Some(v.into());
            }
            SystemUpdate::CleanAdmin(_) => {
                self.admin = None;
            }
        }
        Ok(())
    }
}
