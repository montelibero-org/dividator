pub mod auth;
pub mod types;

use auth::guard_auth;
use auth::types::Permission;
use dividator::state::{K1};
use figment::Figment;
use rocket::fairing::AdHoc;
use rocket::fs::FileServer;
use rocket::http::CookieJar;
use rocket::response::Redirect;
use rocket::serde::json::Json;
use rocket::State;
use rocket::{get, routes};
use rocket_dyn_templates::Template;
use rocket_okapi::{openapi, openapi_get_routes, swagger_ui::*};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, Notify};
use types::*;

#[openapi(tag = "ping")]
#[get("/ping")]
fn ping() -> Json<()> {
    Json(())
}

#[openapi(skip)]
#[get("/")]
async fn index(
    db: &State<DataBase>,
    cookies: &CookieJar<'_>,
    cache_mutex: &State<AuthCache>,
    hedge_cache: &State<SystemCache>,
) -> Result<Template, Redirect> {
    guard_auth(
        db,
        cookies,
        cache_mutex.deref().clone(),
        &vec![Permission::Admin],
        async move {
            let db = db.lock().await;
            let state = db.get().await;
            
            let context = HashMap::from([
                ("title", Cow::Borrowed("Dashboard")),
                ("parent", Cow::Borrowed("base")),
                ("signout", Cow::Borrowed("true")),
            ]);
            Template::render("index", context)
        },
    )
    .await
}

pub async fn serve_api(
    start_notify: Arc<Notify>,
    api_config: Figment,
    db: DataBase,
    hedge_cache: SystemCache,
) -> Result<(), Box<dyn std::error::Error>> {
    let on_ready = AdHoc::on_liftoff("API Start!", |_| {
        Box::pin(async move {
            start_notify.notify_one();
        })
    });
    let domain: String = api_config.extract_inner("domain")?;
    let static_path: PathBuf = api_config.extract_inner("static_path").unwrap();
    let auth_cache = Arc::new(Mutex::new(auth::cache::Cache::default()));
    let (k1_sender, _): (broadcast::Sender<K1>, broadcast::Receiver<K1>) = broadcast::channel(1024);
    let _ = rocket::custom(api_config)
        .mount("/", FileServer::from(static_path))
        .mount(
            "/",
            openapi_get_routes![
                ping,
                auth::routes::index_handler,
                auth::routes::get_qrcode_endpoint,
                auth::routes::get_qrcode_admin_endpoint,
            ],
        )
        .mount(
            "/",
            routes![
                index,
                auth::routes::init,
                auth::routes::signin,
                auth::routes::signin_poll,
                auth::routes::signout,
            ],
        )
        .mount(
            "/swagger/",
            make_swagger_ui(&SwaggerUIConfig {
                url: "../openapi.json".to_owned(),
                ..Default::default()
            }),
        )
        .attach(Template::fairing())
        .attach(on_ready)
        .manage(domain)
        .manage(auth_cache)
        .manage(hedge_cache)
        .manage(db)
        .manage(k1_sender)
        .launch()
        .await?;
    Ok(())
}
