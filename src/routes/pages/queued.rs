use super::super::{HtmlTemplate, AppState};

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;
use axum::{
    extract::State, response::IntoResponse,
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::{info, error};


#[derive(Template, Debug)]
#[template(path = "pages/queued.html")]
struct QueuedPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>
}
#[tracing::instrument]
pub async fn queued(
    State(app): State<Arc<Mutex<AppState>>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("[ Got request to build running page...]");

    // Extract the username from the session
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            error!(%e, "Couldn't get username from session!");
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;

    // Get all running jobs
    let jobs = if let Some(_) = username {
        app.lock()
            .await
            .db
            .get_all_jobs(
                Some(&"Q".to_string()),
                None,
                None,
                None,
                None,
                false
            )
            .map_err(|e| {
                error!(%e, "Couldn't get all jobs!");
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
    } else {
        app.lock()
            .await
            .db
            .get_all_jobs(
                Some(&"Q".to_string()),
                None,
                None,
                None,
                None,
                true
            )
            .map_err(|e| {
                error!(%e, "Couldn't get all jobs!");
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
    };

    // Build jobs and template
    let jobs = jobs.into_iter().rev().collect();
    let template = QueuedPageTemplate {
        username,
        title: String::from("Queued Jobs - CRCD Batchmon"),
        header: String::from("All Queued Jobs on Metis"),
        alert: None,
        jobs,
    };

    Ok(HtmlTemplate(template))
}