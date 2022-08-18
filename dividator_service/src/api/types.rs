use dividator::db::{AppendDb, Postgres};
use dividator::state::SystemState;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shortcase for database behind mutex
pub type DataBase = Arc<Mutex<AppendDb<Postgres<SystemState>>>>;
/// Shortcase for in memory cache for auth sessions
pub type AuthCache = Arc<Mutex<crate::api::auth::cache::Cache>>;
/// Shortcase for in memory cache for hedging
pub type SystemCache = Arc<Mutex<dividator::cache::Cache>>;