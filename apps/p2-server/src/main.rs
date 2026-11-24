//! P2 Server (API) — axum + sqlx + socket.io `/stream`.
//!
//! Eski NestJS sunucusunun (Electron dönemi) yerini alır: aynı PostgreSQL şeması, aynı REST/socket.io
//! sözleşmesi ve aynı JWT biçimi; göçler p2-server-ops içine gömülüdür (`crates/p2-server-ops/migrations`).
//!
//! Ortam değişkenleri: DATABASE_URL, JWT_ACCESS_SECRET, JWT_REFRESH_SECRET, PORT (4000), HOST (0.0.0.0).
//!
//! Komut satırı (cli.rs): `--p2-server`/`--headless` (Windows hizmeti / systemd: server.json + göçler + sunum),
//! `migrate`, `backup [create] [DOSYA]`, `restore DOSYA` / `backup restore DOSYA`.

mod activity;
mod admin;
mod auth;
mod calls;
mod community;
mod channel_permissions;
mod cli;
mod ctx;
mod direct;
mod directory;
mod discover;
mod documents;
mod error;
mod friends;
mod groups;
mod instance;
mod notes;
mod organizations;
mod perm;
mod preferences;
mod project_hub;
mod projects;
mod realtime;
mod releases;
mod roles;
mod scope;
mod screenshots;
mod server_admin;
mod tasks;
mod totp;
mod updates;
mod uploads;
mod user_groups;
mod validate;

use std::sync::{Arc, Mutex, OnceLock};

use axum::Router;
use axum::http::Method;
use axum::routing::{get, patch, post};
use socketioxide::SocketIo;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

pub struct Config {
    pub access_secret: String,
    pub refresh_secret: String,
    pub version: String,
}

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub cfg: Arc<Config>,
    pub presence: Arc<Mutex<realtime::Presence>>,
    /// Açık sesli/görüntülü aramalar (calls.rs).
    pub calls: Arc<Mutex<calls::Registry>>,
    /// socket.io sunucusu; durum nesnesi ondan önce kurulduğu için sonradan doldurulur.
    pub io: Arc<OnceLock<SocketIo>>,
    /// `p2-server.ini`: sunucu kimliği, sunucu tarayıcısı, TURN (instance.rs).
    pub instance: Arc<instance::Instance>,
    /// Sunucu tarayıcısı dizini (yalnız merkezi sunucuda kullanılır; directory.rs).
    pub directory: Arc<Mutex<directory::Directory>>,
}

fn env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("{name} ortam değişkeni gerekli"))
}

fn main() -> anyhow::Result<()> {
    // Alt komutlar ve hizmet kipi tokio çalışma zamanından önce işlenir (ortam değişkenleri burada kurulur).
    if let Some(code) = cli::run()? {
        std::process::exit(code);
    }
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(serve())
}

async fn serve() -> anyhow::Result<()> {
    let cfg = Arc::new(Config {
        access_secret: env("JWT_ACCESS_SECRET")?,
        refresh_secret: env("JWT_REFRESH_SECRET")?,
        version: std::env::var("P2_SERVER_VERSION").unwrap_or_else(|_| "dev".into()),
    });
    let db = PgPoolOptions::new().max_connections(20).connect(&env("DATABASE_URL")?).await?;

    let instance = instance::Instance::load();
    let directory = Arc::new(Mutex::new(directory::Directory::load(&instance.data_dir)));
    let state = AppState { db, cfg, presence: Default::default(), calls: Default::default(), io: Default::default(), instance, directory };
    let (io_layer, io) = SocketIo::builder().with_state(state.clone()).build_layer();
    io.ns("/stream", realtime::on_connect);
    state.io.set(io).ok();
    // NestJS onModuleInit karşılığı: sürüm deposu (tmp süpürme, GC) ve güncelleme klasörü izleyicisi.
    releases::start(&state);
    updates::start(&state);
    server_admin::start(&state);
    instance::start(&state);

    // NestJS: enableCors({ origin: true, credentials: false }) — isteğin kökeni yansıtılır.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_methods([Method::GET, Method::HEAD, Method::PUT, Method::PATCH, Method::POST, Method::DELETE]);

    let app = Router::new()
        .route("/health", get(auth::health))
        .route("/setup/status", get(auth::setup_status))
        .route("/setup/admin", post(auth::setup_admin))
        .route("/auth/me", get(auth::me))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/2fa/setup", post(auth::two_factor_setup))
        .route("/auth/2fa/enable", post(auth::two_factor_enable))
        .route("/auth/2fa/disable", post(auth::two_factor_disable))
        .route("/auth/profile", patch(auth::profile))
        .merge(activity::routes())
        .merge(admin::routes())
        .merge(channel_permissions::routes())
        .merge(community::routes())
        .merge(direct::routes())
        .merge(directory::routes())
        .merge(instance::routes())
        .merge(discover::routes())
        .merge(documents::routes())
        .merge(friends::routes())
        .merge(groups::routes())
        .merge(notes::routes())
        .merge(organizations::routes())
        .merge(preferences::routes())
        .merge(project_hub::routes())
        .merge(projects::routes())
        .merge(releases::routes())
        .merge(roles::routes())
        .merge(screenshots::routes())
        .merge(server_admin::routes())
        .merge(tasks::routes())
        .merge(updates::routes())
        .merge(uploads::routes())
        .merge(user_groups::routes())
        .fallback(error::not_found)
        // main.ts: json gövde sınırı 340 MB (base64 dosyalar).
        .layer(axum::extract::DefaultBodyLimit::max(340 * 1024 * 1024))
        .layer(io_layer)
        .layer(cors)
        .with_state(state);

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    tracing::info!("P2 sunucusu (Rust) {host}:{port} dinliyor");
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
    Ok(())
}
