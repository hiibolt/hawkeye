use super::super::AppState;
use super::{try_render_template, timestamp_field_to_date, Toolkit};

use std::collections::HashSet;
use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use axum::response::Response;
use axum::{
    extract::{Query, State},
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::{error, info, warn};


#[derive(Template, Debug)]
#[template(path = "pages/stats.html")]
struct StatsPageTemplate<'a> {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,

    job: Option<(
        BTreeMap<String, String>,
        Vec<BTreeMap<String, String>>
    )>,
    jobs: Vec<BTreeMap<String, String>>,
    url_prefix: &'a str,
    
    toolkit: Toolkit
}
#[tracing::instrument]
pub async fn stats(
    State(app): State<Arc<AppState>>,
    Query(params): Query<BTreeMap<String, String>>,
    session: Session,
) -> Result<Response, (StatusCode, String)> {
    info!("[ Got request to build the stats page...]");

    // Extract the username from the session 
    //  and the query parameters
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            error!(%e, "Couldn't get username from session!");
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;
    let id_query = params.get("id")
        .and_then(|st| Some(st.to_owned()));

    // Get all running jobs
    let mut job = if let Some(_) = username {
        if let Some(ref id) = id_query {
            let id = id.parse::<i32>()
                .map_err(|e| {
                    error!(%e, "Failed to parse ID!");
                    (StatusCode::BAD_REQUEST, "Failed to parse ID!".to_string())
                })?;

            let mut job = app.db
                .get_job(id)
                .await
                .map_err(|e| {
                    error!(%e, "Couldn't get job!");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get job!".to_string())
                })?;
            let stats = app.db
                .get_job_stats(id)
                .await
                .map_err(|e| {
                    error!(%e, "Couldn't get job stats!");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get job stats!".to_string())
                })?;

            if let Some(end_time_str_ref) = job.get_mut("start_time") {
                timestamp_field_to_date(end_time_str_ref);
            }
            if let Some(end_time_str_ref) = job.get_mut("end_time") {
                timestamp_field_to_date(end_time_str_ref);
            }

            Some((job, stats))
        } else {
            warn!("No user query provided!");
            None
        }
    } else {
        None
    };

    // Get the status of the job and the current timestamp
    let status = if let Some(ref job_stats_pair) = job {
        job_stats_pair.0.get("state")
            .map(|s| s.to_owned())
            .unwrap_or_else(|| "?".to_string())
    } else {
        "?".to_string()
    };
    let current_timestamp = chrono::Local::now()
        .format("%b %e, %Y at %l:%M%p")
        .to_string();

    if let Some(ref mut job) = job {
        let owner = job.0.get("owner")
            .ok_or_else(||
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "No owner found for job!".to_string()
                )
            )?;

        let groups_cache = app.db
            .get_groups_cache()
            .await;
    
        job.0.insert(
            String::from("project"),
            groups_cache.get(owner)
                .unwrap_or(&HashSet::new())
                .into_iter()
                .next()
                .and_then(|st| Some(st.to_owned()))
                .unwrap_or(String::from("None"))
        );
    }

    // Build template
    let template = StatsPageTemplate {
        alert: if let Some(_) = username {
            if let None = id_query {
                Some("No job ID provided!".to_string())
            } else {
                None
            }
        } else {
            Some("You are not logged in!".to_string())
        },
        username,
        title: String::from("Job Stats - CRCD Batchmon"),
        header: if let Some(ref id) = id_query {
            format!("Extended Job Stats - Job ID {id} ({status}) on {current_timestamp}")
        } else {
            String::from("Job Stats")
        },

        job,
        jobs: vec!(),
        url_prefix: &app.url_prefix,

        toolkit:Toolkit
    };

    try_render_template(&template)
}