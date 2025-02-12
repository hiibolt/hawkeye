use super::super::{HtmlTemplate, AppState};
use super::{div_two_i32s_into_f32, timestamp_to_date, to_i32};

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;
use axum::{
    extract::{Query, State}, response::IntoResponse,
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::{error, info, warn};


#[derive(Template, Debug)]
#[template(path = "pages/stats.html")]
struct StatsPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,

    job: Option<(
        BTreeMap<String, String>,
        Vec<BTreeMap<String, String>>
    )>,
    
    div_two_i32s_into_f32: fn(&&String, &&String) -> Result<f32>,
    timestamp_to_date: fn(&&String) -> String,
    to_i32: fn(&&String) -> Result<i32>
}
#[tracing::instrument]
pub async fn stats(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<BTreeMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
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
    let job = if let Some(_) = username {
        if let Some(ref id) = id_query {
            let id = id.parse::<i32>()
                .map_err(|e| {
                    error!(%e, "Failed to parse ID!");
                    (StatusCode::BAD_REQUEST, "Failed to parse ID!".to_string())
                })?;

            let job = app.lock()
                .await
                .db
                .get_job(id)
                .map_err(|e| {
                    error!(%e, "Couldn't get job!");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get job!".to_string())
                })?;
            let stats = app.lock()
                .await
                .db
                .get_job_stats(id)
                .map_err(|e| {
                    error!(%e, "Couldn't get job stats!");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get job stats!".to_string())
                })?;

            Some((job, stats))
        } else {
            warn!("No user query provided!");
            None
        }
    } else {
        None
    };

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
            format!("Extended Job Stats - Job ID {id}")
        } else {
            String::from("Job Stats")
        },

        job,

        div_two_i32s_into_f32,
        timestamp_to_date,
        to_i32
    };

    Ok(HtmlTemplate(template))
}