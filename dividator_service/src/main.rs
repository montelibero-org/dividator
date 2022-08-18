pub mod api;

use crate::api::serve_api;
use clap::Parser;
use dividator::db::create_db_pool;
use dividator::db::{AppendDb, Postgres};
use dividator::state::{SystemState, SystemUpdate};
use futures::future::{AbortHandle, Abortable};
use log::*;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Parser, Debug, Clone)]
#[clap(about, version, author)]
#[clap(propagate_version = true)]
pub struct Args {
    /// PostgreSQL connection string
    #[clap(
        long,
        short,
        default_value = "postgres://dividator:dividator@127.0.0.1:5436/dividator",
        env = "DATABASE_URL"
    )]
    dbconnect: String,

    /// For private internal API. IP adress to listen to incoming connections.
    /// You have to protect that API by external tools like reverse proxy or only
    /// localhost connections.
    #[clap(long, default_value = "127.0.0.1", env = "INTERNAL_HOST")]
    internal_host: String,
    /// For private internal API. Port to listen incoming connection.
    /// You have to protect that API by external tools like reverse proxy or only
    /// localhost connections.
    #[clap(long, default_value = "8100", env = "INTERNAL_PORT")]
    internal_port: u16,

    /// IP adress to listen to incoming connections
    #[clap(long, short, default_value = "127.0.0.1", env = "HOST")]
    host: String,
    /// Port to listen incoming connection for web UI
    #[clap(long, short, default_value = "8099", env = "PORT")]
    port: u16,
    /// Which public domain use for URLs
    #[clap(long, env = "HOST_DOMAIN")]
    host_domain: Option<String>,
    /// Path to HTML, CSS, JS static files to serve
    #[clap(long, env = "STATIC_PATH")]
    static_path: Option<PathBuf>,
    /// Path to dynamic templates for pages
    #[clap(long, env = "TEMPLATE_PATH")]
    template_path: Option<PathBuf>,
    /// Base64 encoded 64 byte secret key for encoding cookies. Required in release profile.
    #[clap(long, env = "COOKIES_SECRET_KEY", hide_env_values = true)]
    cookies_secret_key: Option<String>,
    /// If the flag set to true, cleans admin information on startup.
    /// That allows to reassign admin account without dropping database.
    #[clap(long)]
    clean_admin: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );
    let default_secret_key =
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
            .to_owned();
    let figment_public = rocket::Config::figment()
        .merge((
            "domain",
            args.host_domain
                .unwrap_or(format!("http://127.0.0.1:{}", args.port).to_owned()),
        ))
        .merge((
            "static_path",
            args.static_path
                .unwrap_or(PathBuf::from(rocket::fs::relative!("./static/"))),
        ))
        .merge((
            "template_dir",
            args.template_path
                .unwrap_or(PathBuf::from(rocket::fs::relative!("./templates/"))),
        ))
        .merge((
            "secret_key",
            args.cookies_secret_key
                .clone()
                .unwrap_or(default_secret_key.clone()),
        ))
        .merge(("address", args.host))
        .merge(("port", args.port));

    info!("Connecting to database");
    let pool = create_db_pool(&args.dbconnect).await?;
    let mut adb = AppendDb::new(Postgres::new(pool), SystemState::default());
    info!("Loading database");
    adb.load().await?;
    if args.clean_admin {
        info!("Cleaning admin account...");
        adb.update(SystemUpdate::CleanAdmin(())).await?;
        info!("Admin account cleaned. Please, restart without the flag --clean-admin");
    } else {
        let db = Arc::new(Mutex::new(adb));
        let cache = Arc::new(Mutex::new(dividator::cache::Cache::default()));

        info!("Starting listening...");
        let start_notify_public = Arc::new(Notify::new());
        let public_api_fut = serve_api(start_notify_public, figment_public, db.clone(), cache);
        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        ctrlc::set_handler(move || abort_handle.abort()).expect("Error setting Ctrl-C handler");
        let joined_fut = public_api_fut;
        Abortable::new(joined_fut, abort_registration).await??;
    }
    Ok(())
}
