mod parsing;
mod db;
mod remote;
mod daemons;
mod routes;

use db::lib::*;
use daemons::{jobs::jobs_daemon, groups::groups_daemon};
use routes::{
    jobs::jobs_handler,
    stats::stats_handler,
    auth::{login_handler, logout_handler},
    AppState
};
use remote::auth::verify_login;

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use axum::{
    routing::{get, post}, Router
};
use colored::Colorize;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use time::Duration;


#[tokio::main]
async fn main() -> Result<()> {
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        remote_username: std::env::var("REMOTE_USERNAME")
            .context("Missing `REMOTE_USERNAME` environment variable!")?,
        remote_hostname: std::env::var("REMOTE_HOSTNAME")
            .context("Missing `REMOTE_HOSTNAME` environment variable!")?,
        db: DB::new(
            &std::env::var("DB_PATH")
                .context("Missing `DB_PATH` environment variable!")?
        ).context("Failed to establish connection to DB!")?
    }));
    
    eprintln!("{}", "[ Starting daemons... ]".green());
    tokio::spawn(jobs_daemon(state.clone()));
    tokio::spawn(groups_daemon(state.clone()));
    eprintln!("{}", "[ Daemons started! ]".green());

    // Create the Session store and layer
    let session_store = MemoryStore::default();
    // E.g. sessions expire after 30 minutes of inactivity
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // true requires HTTPS; set appropriately
        .with_expiry(Expiry::OnInactivity(Duration::minutes(30)));

    // Build auth router
    let auth_routes = Router::new()
        .route("/login", post(login_handler))
        .route("/logout", post(logout_handler))
        .with_state(state.clone());

    // Build the V1 API router
    let api_v1 = Router::new()
        .route("/jobs", get(jobs_handler))
        .route("/stats", get(stats_handler))
        .nest("/auth", auth_routes)
        .with_state(state.clone());

    // Nest the API into the general app router
    let app = Router::new()
        .nest("/api/v1", api_v1)
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
