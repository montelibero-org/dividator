use clap::Parser;
use log::*;
use std::error::Error;
use sqlx::postgres::{PgPoolOptions, Postgres as PG};

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
}

type Pool = sqlx::Pool<PG>;

pub async fn create_db_pool(conn_string: &str) -> Result<Pool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(conn_string)
        .await?;

    info!("Applying migrations");
    sqlx::migrate!("../dividator/migrations").run(&pool).await?;

    Ok(pool)
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    info!("Connecting to database");
    let _ = create_db_pool(&args.dbconnect).await?;

    Ok(())
}
