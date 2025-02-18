use super::super::{HtmlTemplate, AppState};
use super::{sort_jobs, to_i32, TableEntry, TableStat, add_exit_status_tooltip};

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;
use axum::{
    extract::{Query, State}, response::IntoResponse
};
use tower_sessions::Session;
use axum::http::StatusCode;
use askama::Template;
use tracing::{info, error};



#[derive(Template, Debug)]
#[template(path = "pages/completed.html")]
struct CompletedPageTemplate {
    username: Option<String>,
    needs_login: bool,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    table_entries: Vec<TableEntry>,

    user_query: Option<String>,
    date_query: Option<String>,

    to_i32: fn(&&String) -> Result<i32>
}
#[tracing::instrument]
pub async fn completed(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("[ Got request to build completed page...]");

    // Unpack username and query parameters
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            error!(%e, "Couldn't get username from session!");
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;
    let user_query = params.get("user")
        .and_then(|st| Some(st.to_owned()))
        .or(username.clone());
    let date_query = params.get("date")
        .and_then(|st| Some(st.to_owned()));

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

    // Get the jobs
    let mut jobs = if let Some(_) = username {    
        app.lock()
            .await
            .db
            .get_user_jobs(
                &user_query.clone().expect("Unreachable"),
                Some(&"E".to_string()),
                None,
                None,
                None, 
                Some(&timestamp_filter)
            )
            .map_err(|e| {
                error!(%e, "Couldn't get all jobs!");
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
    } else {
        vec!()
    };

    // Insert the number of required nodes
    jobs = jobs.into_iter()
        .map(|mut job| {
            job.insert(
                String::from("req_nodes"),
                job["nodes"].split(',').collect::<Vec<&str>>().len().to_string()
            );

            job
        })
        .rev()
        .collect();

    // Sort the jobs by any sort and reverse queries
    sort_jobs(
        &mut jobs,
        params.get("sort"),
        params.get("reverse"),
        username.is_some()
    );

    // Tweak data to be presentable and add tooltips for efficiencies
    let table_stats = vec!(
        TableStat::RsvdCpus,
        TableStat::NodesChunks,
        TableStat::RsvdMem,
        TableStat::EndTime,
        TableStat::UsedMemPerCore,
        TableStat::UsedMem,
        TableStat::CpuTime,
        TableStat::WalltimeEfficiency,
        TableStat::CpuEfficiency,
        TableStat::MemEfficiency
    );
    jobs = jobs.into_iter()
        .map(|mut job| {
            // Add tooltip for exit status
            add_exit_status_tooltip(&mut job);

            for table_stat in table_stats.iter() {
                table_stat.adjust_job(&mut job);
            }

            job
        })
        .collect();

    // Build the header and template
    let header = if let Some(ref user_query) = user_query {
        format!(
            "Completed Jobs for '{}' on Metis ({})",
            user_query,
            date_query.clone().unwrap_or("month".to_string())
        )
    } else {
        String::from("Completed Jobs on Metis")
    };
    let template = CompletedPageTemplate {
        // Cool compiler magic here :3c (avoids cloning)
        alert: if username.is_none() { Some("You are not logged in!".to_string()) } else { None },
        username,
        needs_login: true,
        title: String::from("Completed Jobs - CRCD Batchmon"),
        header,
        jobs,

        table_entries: table_stats.into_iter()
            .map(|table_stat| table_stat.into() )
            .collect(),

        user_query,
        date_query,

        to_i32
    };

    Ok(HtmlTemplate(template))
}