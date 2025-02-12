use super::super::{HtmlTemplate, AppState};
use super::{sort_jobs, timestamp_to_date, to_i32, TableEntry};

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

    // Tweak data to be presentable
    jobs = jobs.into_iter()
        .map(|mut job| {
            job.insert(
                String::from("used_mem_per_cpu"),
                ( job.get("used_mem")
                    .and_then(|st| st.parse::<f32>().ok())
                    .unwrap_or(0f32) /
                job.get("req_cpus")
                    .and_then(|st| st.parse::<f32>().ok())
                    .unwrap_or(1f32) )
                    .to_string()
            );
            job.insert(
                String::from("nodes/chunks"),
                format!("{}/{}", 
                    job.get("nodes").unwrap_or(&"".to_string())
                        .split(',').collect::<Vec<&str>>().len(),
                    job.get("chunks").unwrap_or(&"0".to_string())
                )
            );
            if let Some(end_time_str_ref) = job.get_mut("end_time") {
                *end_time_str_ref = timestamp_to_date(&&*end_time_str_ref);
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
        title: String::from("Completed Jobs - CRCD Batchmon"),
        header,
        jobs,
        table_entries: vec![
            ("# of CPUs", "req_cpus", "req_cpus", "", false),
            ("Nodes/Chunks", "chunks", "nodes/chunks", "", false),
            ("Requested Mem", "req_mem", "req_mem", "GB", false),
            ("End Date", "end_time", "end_time", "", false),
            ("Used Mem/Core", "NOT_SORTABLE", "used_mem_per_cpu", "GB", false),
            ("Used Mem", "used_mem", "used_mem", "GB", false),
            ("Used Walltime", "used_walltime", "used_walltime", "", false),
            ("Req/Used CPU", "NOT_SORTABLE", "cpu_efficiency", "", true),
            ("Req/Used Mem", "NOT_SORTABLE", "mem_efficiency", "", true),
            ("Req/Used Walltime", "NOT_SORTABLE", "walltime_efficiency", "", true)
        ].into_iter()
            .map(|(name, sort_by, value, value_units, colored)| TableEntry {
                name: name.to_string(),
                sort_by: sort_by.to_string(),
                value: value.to_string(),
                value_unit: value_units.to_string(),
                colored
            })
            .collect(),

        user_query,
        date_query,

        to_i32
    };

    Ok(HtmlTemplate(template))
}