use super::super::AppState;
use super::{sort_build_parse, try_render_template, Toolkit, TableEntry, TableStat, TableStatType, PageType};

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use anyhow::Result;
use axum::{
    extract::{Query, State}, response::Response
};
use tower_sessions::Session;
use axum::http::StatusCode;
use askama::Template;
use tracing::{error, info};



#[derive(Template, Debug)]
#[template(path = "pages/completed.html")]
struct CompletedPageTemplate<'a> {
    username: Option<String>,
    needs_login: bool,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    table_entries: Vec<TableEntry>,

    user_query: Option<String>,
    date_query: Option<String>,
    url_prefix: &'a str,

    page_type: PageType,
    toolkit: Toolkit
}
#[tracing::instrument]
pub async fn completed(
    State(app): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<Response, (StatusCode, String)> {
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
    let adjusted_timestamp = match date_query {
        Some(ref date_query) => {
            let date_query = date_query.to_lowercase();
            let now = chrono::Local::now();

            match date_query.as_str() {
                "day" => now.timestamp() - 86400,
                "month" => now.timestamp() - 2592000,
                "year" => now.timestamp() - 31536000,
                "all" => now.timestamp() - 315360000,
                _ => now.timestamp() - 2592000
            }
        },
        None => chrono::Local::now().timestamp() - 2592000
    };

    // Make it a human readable date
    let adjusted_date = chrono::DateTime::from_timestamp(adjusted_timestamp, 0)
        .ok_or_else(|| {
            error!("Couldn't convert timestamp to date!");

            (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't convert timestamp to date!".to_string())
        })?;
    let adjusted_date = adjusted_date.format("%b %e, %Y at %l:%M%p").to_string();

    // Get the jobs
    let mut jobs = if let Some(_) = username {    
        app.db
            .get_user_jobs(
                &user_query.clone().expect("Unreachable"),
                Some(&"E".to_string()),
                None,
                None,
                None, 
                Some(&adjusted_timestamp.to_string())
            )
            .await
            .map_err(|e| {
                error!(%e, "Couldn't get all jobs!");
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
    } else {
        vec!()
    };

    // Tweak data to be presentable and add tooltips for efficiencies
    let groups_cache = app.db
        .get_groups_cache()
        .await;
    let (table_entries, errors) = sort_build_parse(
        groups_cache,
        vec!(
            TableStat::JobID,
            TableStat::JobOwner,
            TableStat::RsvdCpus,
            TableStat::NodesChunks,
            TableStat::RsvdMem,
            TableStat::EndTime,
            TableStat::UsedMemPerCore,
            TableStat::UsedMem,
            TableStat::CpuTime,
            TableStat::ElapsedWalltimeColored,
            TableStat::CpuEfficiency,
            TableStat::MemEfficiency,
            TableStat::ExitStatus,
            TableStat::More
        ),

        &mut jobs,
        &params,
        username.clone()
    );
    let url_prefix = &app.url_prefix;

    // Build the template
    let template = CompletedPageTemplate {
        jobs,
        alert: if username.is_some() {
                errors
            } else {
                Some("You are not logged in!".to_string())
            },
        username,
        needs_login: true,
        title: format!("Completed Jobs - CRCD Batchmon"),
        header: if let Some(ref user_query) = user_query {
            format!(
                "Completed Jobs for '{}' on Metis - Since {}",
                user_query,
                adjusted_date
            )
        } else {
            String::from("Completed Jobs on Metis")
        },

        table_entries,

        user_query,
        date_query,
        url_prefix,

        toolkit:Toolkit,
        page_type: PageType::Completed
    };

    try_render_template(&template)
}