use super::super::{HtmlTemplate, AppState};
use super::{TableEntry, to_i32, sort_jobs};
use std::collections::HashMap;
use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use axum::extract::Query;
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
    needs_login: bool,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    table_entries: Vec<TableEntry>,

    to_i32: fn(&&String) -> Result<i32>
}
#[tracing::instrument]
pub async fn queued(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
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
    let mut jobs = if let Some(_) = username {
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
    
    // Sort the jobs by any sort and reverse queries
    sort_jobs(
        &mut jobs,
        params.get("sort"),
        params.get("reverse"),
        username.is_some()
    );

    // Build jobs and template
    let jobs = jobs.into_iter().rev().collect();
    let template = QueuedPageTemplate {
        username,
        needs_login: false,
        title: String::from("Queued Jobs - CRCD Batchmon"),
        header: String::from("All Queued Jobs on Metis"),
        alert: None,
        jobs,
        table_entries: vec![
            ("Job Name", "name", "name", "", false),
            ("Queue", "queue", "queue", "", false),
            ("Walltime", "req_walltime", "req_walltime", "", false),
            ("# of CPUs", "req_cpus", "req_cpus", "", false),
            ("# of GPUs", "req_gpus", "req_gpus", "", false),
            ("Memory", "req_mem", "req_mem", "GB", false)
        ].into_iter()
            .map(|(name, sort_by, value, value_units, colored)| TableEntry {
                name: name.to_string(),
                sort_by: sort_by.to_string(),
                value: value.to_string(),
                value_unit: value_units.to_string(),
                colored
            })
            .collect(),

        to_i32
    };

    Ok(HtmlTemplate(template))
}