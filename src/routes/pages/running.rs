use crate::routes::ClusterStatus;

use super::super::AppState;
use super::{try_render_template, TableEntry, TableStat, TableStatType, Toolkit, sort_build_parse};

use std::collections::HashMap;
use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use axum::extract::Query;
use axum::response::Response;
use axum::{
    extract::State,
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::{info, error};

#[derive(Template, Debug)]
#[template(path = "pages/running.html")]
struct RunningPageTemplate<'a> {
    username: Option<String>,
    needs_login: bool,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    table_entries: Vec<TableEntry>,

    cluster_status: Option<ClusterStatus>,
    url_prefix: &'a str,

    toolkit: Toolkit
}
#[tracing::instrument]
pub async fn running(
    State(app): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<Response, (StatusCode, String)> {
    info!("[ Got request to build running page...]");

    // Extract the username from the session
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            error!(%e, "Couldn't get username from session!");
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;

    // Get all running jobs
    let mut jobs = app.db
        .lock().await
        .get_all_jobs(
            Some(vec!("R", "Q")),
            None,
            None,
            None,
            None
        )
        .map_err(|e| {
            error!(%e, "Couldn't get all jobs!");
            (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
        })?;
    
    // Tweak data to be presentable and add tooltips for efficiencies
    let groups_cache = app.db
        .lock().await
        .get_groups_cache();
    let (table_entries, errors) = sort_build_parse(
        groups_cache,
        vec!(
            TableStat::JobID,
            TableStat::JobOwner,
            TableStat::JobName(10),
            TableStat::JobProject,
            TableStat::Status,
            TableStat::StartTime,
            TableStat::Queue,
            TableStat::RsvdTime,
            TableStat::RsvdCpus,
            TableStat::RsvdGpus,
            TableStat::RsvdMem,
            TableStat::ElapsedWalltime,
            TableStat::CpuEfficiency,
            TableStat::MemEfficiency,
            TableStat::More
        ),

        &mut jobs,
        &params,
        username.clone()
    );
    let url_prefix = &app.url_prefix;
    
    // Build template
    let template = RunningPageTemplate {
        username,
        needs_login: false,
        title: String::from("Cluster Overview - Batch Job Monitor"),
        header: format!(
            "Submitted Jobs Status - Metis Cluster - {}",
            chrono::Local::now()
                .format("%b %e, %Y at %l:%M%p")
        ),
        jobs,
        alert: errors,
        table_entries,

        cluster_status: *app.status.read().await,
        url_prefix,

        toolkit: Toolkit
    };

    try_render_template(&template)
}