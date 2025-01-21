mod parsing;
mod db;
mod remote;
mod daemons;
mod routes;

use db::lib::*;
use daemons::jobs::*;
use routes::{jobs::jobs_handler, stats::stats_handler, AppState};

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use axum::{
    routing::get, Router
};
use colored::Colorize;

#[tokio::main]
async fn main() -> Result<()> {
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        db: DB::new(
            &std::env::var("DB_PATH")
                .context("Missing `DB_PATH` environment variable!")?
        ).context("Failed to establish connection to DB!")?
    }));
    
    jobs_daemon(state.clone()).await;

    // Build the V1 API router
    let api_v1 = Router::new()
        .route("/jobs", get(jobs_handler))
        .route("/stats", get(stats_handler))
        .with_state(state.clone());

    // Nest the API into the general app router
    let app = Router::new()
        .nest("/api/v1", api_v1);

    // Start the server
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    eprintln!("{}", format!("[ Starting Hawkeye Backend on {port}... ]").green());
    let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{port}")).await
        .context("Couldn't start up listener!")?;
    axum::serve(listener, app).await
        .context("Could't serve the API!")?;

    Ok(())
}
