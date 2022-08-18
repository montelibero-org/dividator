use sqlx::postgres::{PgPoolOptions, Postgres as PG};

pub use append_db::db::AppendDb;
pub use append_db_postgres::backend::Postgres;

pub type Pool = sqlx::Pool<PG>;

pub async fn create_db_pool(conn_string: &str) -> Result<Pool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(conn_string)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
