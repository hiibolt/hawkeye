use super::super::AppState;
use super::{try_render_template, TableEntry, TableStat, TableStatType, Toolkit, sort_build_parse};

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use anyhow::Result;
use axum::response::Response;
use tokio::sync::Mutex;
use axum::{
    extract::{Query, State},
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::{info, error};


#[derive(Template, Debug)]
#[template(path = "pages/search.html")]
struct SearchPageTemplate {
    username: Option<String>,
    needs_login: bool,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    table_entries: Vec<TableEntry>,

    state_query: Option<String>,
    queue_query: Option<String>,
    user_query: Option<String>,
    name_query: Option<String>,
    date_query: Option<String>,
    url_prefix: String,

    toolkit: Toolkit
}
#[tracing::instrument]
pub async fn search(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<Response, (StatusCode, String)> {
    info!("[ Got request to build the search page...]");

    // Extract the session username and query parameters
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            error!(%e, "Couldn't get username from session!");
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;
    let date_query = params.get("date")
        .and_then(|st| Some(st.to_owned()));
    let any_filters = params.get("state").is_some() || 
        params.get("queue").is_some() || 
        params.get("user").is_some() || 
        params.get("name").is_some() || 
        params.get("date").is_some();

    // Convert our date query to a timestamp, using `month`
    //  by default. Options are `day`, `month`, `year`, `all` (10 years)
    let timestamp_filter = if let Some(ref date_query) = date_query {
        let date_query = date_query.to_lowercase();
        let now = chrono::Local::now();
        let timestamp = match date_query.as_str() {
            "day" => now.timestamp() - 86400,
            "month" => now.timestamp() - 2592000,
            "year" => now.timestamp() - 31536000,
            "all" => now.timestamp() - 315360000,
            _ => now.timestamp() - 2592000
        };

        timestamp.to_string()
    } else { // Default to a month
        (chrono::Local::now().timestamp() - 2592000).to_string()
    };

    // Get all running jobs
    let mut jobs = if let Some(_) = username {
        if any_filters {
            app.lock()
                .await
                .db
                .get_all_jobs(
                    params.get("state")
                        .and_then(|st| Some(vec!(st.as_str()))),
                    params.get("queue"),
                    params.get("user"),
                    params.get("name"),
                    Some(&timestamp_filter),
                    false
                )
                .map_err(|e| {
                    error!(%e, "Couldn't get all jobs!");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
                })?
        } else {
            vec!()
        }
    } else {
        vec!()
    };

    // Tweak data to be presentable and add tooltips for efficiencies
    let groups_cache = app.lock().await
        .db
        .get_groups_cache();
    let (table_entries, errors) = sort_build_parse(
        groups_cache,
        vec!(
            TableStat::JobID,
            TableStat::JobOwner,
            TableStat::JobName(10),
            TableStat::Status,
            TableStat::StartTime,
            TableStat::EndTime,
            TableStat::CpuTime,
            TableStat::UsedMemPerCore,
            TableStat::UsedMem,
            TableStat::Queue,
            TableStat::RsvdTime,
            TableStat::RsvdCpus,
            TableStat::RsvdGpus,
            TableStat::RsvdMem,
            TableStat::ElapsedWalltime,
            TableStat::NodesChunks,
            TableStat::ElapsedWalltimeColored,
            TableStat::CpuEfficiency,
            TableStat::MemEfficiency,
            TableStat::ExitStatus,
            TableStat::More,
        ),

        &mut jobs,
        &params,
        username.clone()
    );
    let url_prefix = app.lock().await.url_prefix.clone();

    // Build jobs and template
    let template = SearchPageTemplate {
        alert: if username.is_none() {
                Some("You are not logged in!".to_string())
            } else {
                if errors.is_some() {
                    errors
                } else {
                    if any_filters {
                        None
                    } else {
                        Some("Choose your filters and click search!".to_string())
                    }
                }
            },
        username,
        needs_login: true,
        title: String::from("Search - CRCD Batchmon"),
        header: String::from("Search"),
        jobs,
        table_entries,

        state_query: params.get("state").and_then(|st| Some(st.to_owned())),
        queue_query: params.get("queue").and_then(|st| Some(st.to_owned())),
        user_query: params.get("user").and_then(|st| Some(st.to_owned())),
        name_query: params.get("name").and_then(|st| Some(st.to_owned())),
        date_query,
        url_prefix,

        toolkit:Toolkit
    };

    try_render_template(&template)
}