mod parsing;
mod db;
mod remote;
mod daemons;
mod routes;


use db::lib::*;
use daemons::{groups::groups_daemon, jobs::{jobs_daemon, old_jobs_daemon}};
use routes::AppState;

use std::sync::Arc;

use tokio::sync::RwLock;
use axum::{
    routing::{get, post}, Router
};
use tower_sessions::{cookie::Key, Expiry, MemoryStore, SessionManagerLayer};
use tracing::info;
use tracing_subscriber;


#[tokio::main]
async fn main() -> ! {
    // Initialize the logger
    let file_appender = tracing_appender::rolling::daily("./logs", "daily.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .compact()
        .with_writer(non_blocking)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Write panics to `./logs/panic.log-<timestamp>`
    std::panic::set_hook(Box::new(|panic| {
        let panic_info = format!("{}", panic);

        // Create the filename
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let filename = format!("./logs/panic.log-{}", timestamp);

        // Print the filename to stderr
        eprintln!("[ Panic info written to `{}` ]", filename);
        eprintln!("[ Please report this to the developers! ]");
        eprintln!("[ Full panic info: ]\n{}", panic_info);

        // Write the panic info to the file
        std::fs::write(&filename, panic_info)
            .expect("Failed to write panic info to file!");
    }));

    // Create the shared state
    let url_prefix = std::env::var("URL_PREFIX")
        .unwrap_or(String::new());
    let state: Arc<AppState> = Arc::new(AppState {
        remote_username: std::env::var("REMOTE_USERNAME")
            .expect("Missing `REMOTE_USERNAME` environment variable!"),
        remote_hostname: std::env::var("REMOTE_HOSTNAME")
            .expect("Missing `REMOTE_HOSTNAME` environment variable!"),
        db: DB::new(
            &std::env::var("DB_PATH")
                .expect("Missing `DB_PATH` environment variable!")
        ).expect("Failed to establish connection to DB!"),
        url_prefix: url_prefix.clone(),

        status: RwLock::new(None)
    });
    
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
