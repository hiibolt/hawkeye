mod parsing;
mod db;
mod remote;
mod daemons;
mod routes;


use db::lib::*;
use daemons::{groups::groups_daemon, jobs::{jobs_daemon, old_jobs_daemon}};
use routes::AppState;

use std::sync::Arc;

use tokio::sync::Mutex;
use axum::{
    routing::{get, post}, Router
};
use tower_sessions::{cookie::Key, Expiry, MemoryStore, SessionManagerLayer};
use tracing::info;
use tracing_subscriber;


#[tokio::main]
async fn main() -> ! {
    // Initialize the logger
    tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Create the shared state
    let url_prefix = std::env::var("URL_PREFIX")
        .unwrap_or(String::new());
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        remote_username: std::env::var("REMOTE_USERNAME")
            .expect("Missing `REMOTE_USERNAME` environment variable!"),
        remote_hostname: std::env::var("REMOTE_HOSTNAME")
            .expect("Missing `REMOTE_HOSTNAME` environment variable!"),
        db: DB::new(
            &std::env::var("DB_PATH")
                .expect("Missing `DB_PATH` environment variable!")
        ).expect("Failed to establish connection to DB!"),
        url_prefix: url_prefix.clone(),

        status: None
    }));
    
    info!("[ Starting daemons... ]");
    tokio::spawn(jobs_daemon(state.clone()));
    tokio::spawn(old_jobs_daemon(state.clone()));
    tokio::spawn(groups_daemon(state.clone()));
    info!("[ Daemons started! ]");

    // Create the Session store and layer
    let session_store = MemoryStore::default();
    // E.g. sessions expire after 30 minutes of inactivity
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnSessionEnd)
        .with_secure(true)
        .with_private(
            Key::try_generate()
                .expect("Couldn't generate a session key!")
        ); 

    // Build auth router
    let auth_routes = Router::new()
        .route("/login", post(routes::api::auth::login))
        .route("/logout", post(routes::api::auth::logout))
        .with_state(state.clone());

    // Build the V1 API router
    let api_v1 = Router::new()
        .nest("/auth", auth_routes)
        .with_state(state.clone());

    // Nest the API into the general app router
    let mut app = Router::new()
        .nest(&(url_prefix.clone() + "/api/v1"), api_v1)
        .route(&(url_prefix.clone() + "/"), get(routes::pages::running::running))
        .route(&(url_prefix.clone() + "/login"), get(routes::pages::login::login))
        .route(&(url_prefix.clone() + "/stats"), get(routes::pages::stats::stats))
        .route(&(url_prefix.clone() + "/running"), get(routes::pages::running::running))
        .route(&(url_prefix.clone() + "/completed"), get(routes::pages::completed::completed))
        .route(&(url_prefix.clone() + "/search"), get(routes::pages::search::search))
        .route(&(url_prefix.clone() + "/public/images/favicon.ico"), get(routes::get_favicon));

    if !url_prefix.is_empty() {
        app = app.route(&url_prefix, get(routes::pages::running::running));
    }

    let app = app
        .layer(session_layer)
        .with_state(state.clone());

    // Start the server
    let port = std::env::var("PORT").unwrap_or("5777".to_string());
    loop {
        eprintln!("[ Starting Hawkeye on {port}... ]");
        let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{port}")).await
            .expect("Couldn't start up listener!");
        if let Err(e) = axum::serve(listener, app.clone()).await {
            eprintln!("[ Error: {} ]", e);
        }
    }
}
