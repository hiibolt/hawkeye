use super::super::{HtmlTemplate, AppState};
use super::sort_jobs;

use std::collections::HashMap;
use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
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
#[template(path = "pages/running.html")]
struct RunningPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,

    timestamp_to_date: fn(&&String) -> String,
    to_i32: fn(&&String) -> Result<i32>,
    shorten: fn(&&String) -> String
}
#[tracing::instrument]
pub async fn running(
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
                Some(&"R".to_string()),
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
                Some(&"R".to_string()),
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

    // Build helper functions
    fn timestamp_to_date ( timestamp: &&String ) -> String {
        let timestamp = timestamp.parse::<i64>().unwrap();
        if let Some(date_time) = chrono::DateTime::from_timestamp(timestamp, 0) {
            date_time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        } else {
            String::from("Invalid timestamp!")
        }
    }
    fn to_i32 ( num: &&String ) -> Result<i32> {
        Ok(num.parse::<f64>()
            .context("Failed to parse number!")?
            as i32)
    }
    fn shorten ( text: &&String ) -> String {
        if text.len() > 20 {
            format!("{}...", &text[..20])
        } else {
            text.to_string()
        }
    }
    
    // Sort the jobs by any sort and reverse queries
    sort_jobs(
        &mut jobs,
        params.get("sort"),
        params.get("reverse"),
        username.is_some()
    );

    // Build jobs and template
    let jobs = jobs.into_iter().rev().collect();
    let template = RunningPageTemplate {
        username,
        title: String::from("Running Jobs - CRCD Batchmon"),
        header: String::from("All Running Jobs on Metis"),
        alert: None,
        jobs,

        timestamp_to_date,
        to_i32,
        shorten
    };

    Ok(HtmlTemplate(template))
}