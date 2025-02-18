use crate::routes::ClusterStatus;

use super::super::AppState;
use super::{try_render_template, TableEntry, TableStat, TableStatType, to_i32, shorten, get_field, sort_jobs};

use std::collections::HashMap;
use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use axum::extract::Query;
use axum::response::Response;
use tokio::sync::Mutex;
use axum::{
    extract::State,
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::{info, error};

#[derive(Template, Debug)]
#[template(path = "pages/running.html")]
struct RunningPageTemplate {
    username: Option<String>,
    needs_login: bool,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    table_entries: Vec<TableEntry>,

    cluster_status: Option<ClusterStatus>,

    to_i32: fn(&&String) -> Result<i32>,
    shorten: fn(&&String) -> String,
    get_field: fn(&BTreeMap<String, String>, &str) -> Result<String>
}
#[tracing::instrument]
pub async fn running(
    State(app): State<Arc<Mutex<AppState>>>,
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
    let mut jobs = if let Some(_) = username {
        app.lock()
            .await
            .db
            .get_all_jobs(
                Some(vec!("R", "Q")),
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
                Some(vec!("R", "Q")),
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

    // Tweak data to be presentable and add tooltips for efficiencies
    let table_stats = vec!(
        TableStat::Status,
        TableStat::StartTime,
        TableStat::Queue,
        TableStat::RsvdTime,
        TableStat::RsvdCpus,
        TableStat::RsvdGpus,
        TableStat::RsvdMem,
        TableStat::WalltimeEfficiency,
        TableStat::CpuEfficiency,
        TableStat::MemEfficiency
    );
    let mut errors = Vec::new();
    jobs = jobs.into_iter()
        .map(|mut job| {
            for table_stat in table_stats.iter() {
                if let Err(e) = table_stat.adjust_job(&mut job) {
                    errors.push(e);
                }
                if let Err(e) = table_stat.ensure_needed_field(&mut job) {
                    errors.push(e);
                }
            }

            job
        })
        .rev()
        .collect();
    let errors = errors.iter()
        .map(|e| e.to_string())
        .enumerate()
        .map(|(i, e)| format!("{}. {}", i + 1, e))
        .collect::<Vec<String>>()
        .join("<br>");

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
        jobs: if errors.len() == 0 { jobs } else { Vec::new() },
        alert: (!errors.is_empty()).then_some(format!("<b>There were internal errors!</b><br><br>{errors}")),
        table_entries: table_stats.into_iter()
            .map(|table_stat| table_stat.into() )
            .collect(),

        cluster_status: app.lock().await.status,

        to_i32,
        shorten,
        get_field
    };

    try_render_template(&template)
}