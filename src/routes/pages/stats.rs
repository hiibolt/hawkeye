use super::super::AppState;
use super::{timestamp_field_to_date, try_render_template, TableEntry, TableStat, TableStatType, Toolkit, PageType, sort_build_parse};

use std::collections::{HashMap, HashSet};
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
    tables: Vec<(String, Vec<TableEntry>)>,
    url_prefix: &'a str,
    
    toolkit: Toolkit,
    page_type: PageType
}
#[tracing::instrument]
pub async fn stats(
    State(app): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
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
    let mut job: Option<(
        BTreeMap<String, String>,
        Vec<BTreeMap<String, String>>
    )> = if let Some(_) = username {
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

    let mut jobs = std::iter::once(job.clone())
        .flatten()
        .map(|job_pair| job_pair.0)
        .collect::<Vec<_>>();
    let groups_cache = app.db
        .get_groups_cache()
        .await;
    let mut all_errors: Option<String> = None;
    let tables = vec!(
            ("Metadata", vec!(
                TableStat::JobID,
                TableStat::JobOwner,
                TableStat::JobName(20),
                TableStat::JobProject,
                TableStat::Queue,
                TableStat::Status,
                TableStat::ExitStatus,
            )),
            ("Walltime", vec!(
                TableStat::StartTime,
                TableStat::EndTime,
                TableStat::RsvdTime,
                TableStat::ElapsedWalltime,
            )),
            ("CPU", vec!(
                TableStat::CpuTime,
                TableStat::RsvdCpus,
                TableStat::UsedMemPerCore,
                TableStat::CpuEfficiency,
            )),
            ("Memory", vec!(
                TableStat::UsedMem,
                TableStat::UsedOverRsvdMem,
                TableStat::RsvdMem,
                TableStat::MemEfficiency,
            )),
            ("Other Hardware", vec!(
                TableStat::RsvdGpus,
                TableStat::NodesChunks,
            ))
        )
        .into_iter()
        .map(|(title, stats)| {
            let (table_entries, errors) = sort_build_parse(
                groups_cache.clone(),
                stats,
                &mut jobs,
                &params,
                username.clone()
            );

            if let Some(errors) = errors {
                if let Some(all_errors) = all_errors.as_mut() {
                    *all_errors += " ";
                    all_errors.push_str(&errors);
                } else {
                    all_errors = Some(errors);
                }
            }

            (title.to_string(), table_entries)
        })
        .collect::<Vec<_>>();
    if let Some(ref mut job) = job {
        if let Some(modified_job) = jobs.get(0) {
            job.0 = modified_job.clone();
        }

        let owner = job.0.get("owner")
            .ok_or_else(||
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "No owner found for job!".to_string()
                )
            )?;
    
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
                all_errors
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
        tables,
        url_prefix: &app.url_prefix,

        toolkit:Toolkit,
        page_type: PageType::Stats
    };

    try_render_template(&template)
}