mod parsing;
mod db;
mod remote;
mod daemons;
mod routes;

use db::lib::*;
use daemons::{groups::groups_daemon, jobs::{jobs_daemon, old_jobs_daemon}};
use routes::{
    jobs::jobs_handler,
    stats::stats_handler,
    auth::{login_handler, logout_handler, me_handler},
    AppState
};

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use axum::{
    response::Html, routing::{get, post}, Router
};
use colored::Colorize;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use time::Duration;
use tower_http::{
    services::ServeDir,
    cors::CorsLayer
};
use http::{HeaderValue, Method};


#[tokio::main]
async fn main() -> Result<()> {
    // Check if we're in development mode,
    //  and set the frontend base URL accordingly
    let frontend_base = if std::env::var("DEV_MODE")
        .and_then(|v| Ok(v == "true"))
        .unwrap_or(false)
    {
        eprintln!("{}", "[ Running in development mode! ]".yellow());
        std::env::var("DEV_FRONTEND_BASEURL")
            .context("Missing `DEV_FRONTEND_BASEURL` environment variable!")?
    } else {
        std::env::var("PROD_FRONTEND_BASEURL")
            .context("Missing `PROD_FRONTEND_BASEURL` environment variable!")?
    };

    // Create the shared state
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        remote_username: std::env::var("REMOTE_USERNAME")
            .context("Missing `REMOTE_USERNAME` environment variable!")?,
        remote_hostname: std::env::var("REMOTE_HOSTNAME")
            .context("Missing `REMOTE_HOSTNAME` environment variable!")?,
        db: DB::new(
            &std::env::var("DB_PATH")
                .context("Missing `DB_PATH` environment variable!")?
        ).context("Failed to establish connection to DB!")?,
        frontend_base: frontend_base
    }));
    
    eprintln!("{}", "[ Starting daemons... ]".green());
    tokio::spawn(jobs_daemon(state.clone()));
    tokio::spawn(old_jobs_daemon(state.clone()));
    tokio::spawn(groups_daemon(state.clone()));
    eprintln!("{}", "[ Daemons started! ]".green());

    // Create the Session store and layer
    let session_store = MemoryStore::default();
    // E.g. sessions expire after 30 minutes of inactivity
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::minutes(30)));
    //.with_secure(
    //    std::env::var("DEV_MODE")
    //        .and_then(|v| Ok(v != "true"))
    //        .unwrap_or(true) // true requires HTTPS; set appropriately
    //) // true requires HTTPS; set appropriately

    // Build auth router
    let auth_routes = Router::new()
        .route("/login", post(login_handler))
        .route("/logout", post(logout_handler))
        .route("/me", get(me_handler))
        .with_state(state.clone());

    // Build the V1 API router
    let api_v1 = Router::new()
        .route("/jobs", get(jobs_handler))
        .route("/stats", get(stats_handler))
        .nest("/auth", auth_routes)
        .with_state(state.clone());

    // CORS layer that allows cross-site requests
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(
            state.lock().await.frontend_base
                .parse::<HeaderValue>()
                .context("Invalid frontend origin!")?)
        .allow_credentials(true);

    // Nest the API into the general app router
    let app = Router::new()
        .nest("/api/v1", api_v1)
        .route("/", get(|| async { Html(std::include_str!("../public/html/index.html")) }))
        .route("/index.html", get(|| async { Html(std::include_str!("../public/html/index.html")) }))
        .route("/login.html", get(|| async { Html(std::include_str!("../public/html/login.html")) }))
        .route("/stats.html", get(|| async { Html(std::include_str!("../public/html/stats.html")) }))
        .nest_service("/public", ServeDir::new("public"))
        .layer(cors)
        .layer(session_layer);

    // Start the server
    let port = std::env::var("PORT").unwrap_or("5777".to_string());
    eprintln!("{}", format!("[ Starting Hawkeye Backend on {port}... ]").green());
    let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{port}")).await
        .context("Couldn't start up listener!")?;
    axum::serve(listener, app).await
        .context("Could't serve the API!")?;

    Ok(())
}
