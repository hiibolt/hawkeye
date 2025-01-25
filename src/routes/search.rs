use super::{HtmlTemplate, AppState};

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use axum::{
    extract::{Query, State}, response::IntoResponse
};
use colored::Colorize;
use tower_sessions::Session;
use http::StatusCode;
use askama::Template;


#[derive(Template)]
#[template(path = "search.html")]
struct SearchPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,

    state_query: Option<String>,
    queue_query: Option<String>,
    user_query: Option<String>,
    name_query: Option<String>,
    date_query: Option<String>,

    parse_nodes: fn(&&String) -> String,
    timestamp_to_date: fn(&&String) -> String,
    to_i32: fn(&&String) -> Result<i32>
}
pub async fn search(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    eprintln!("{}", "[ Got request to build running page...]".green());

    // Extract the session username and query parameters
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't get username from session! Error: {e:?}").red());
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
    let jobs = if let Some(_) = username {
        if any_filters {
            app.lock()
                .await
                .db
                .get_all_running_jobs(
                    params.get("state"),
                    params.get("queue"),
                    params.get("user"),
                    params.get("name"),
                    Some(&timestamp_filter),
                    false
                )
                .map_err(|e| {
                    eprintln!("{}", format!("Couldn't get all jobs! Error: {e:?}").red());
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
                })?
        } else {
            vec!()
        }
    } else {
        vec!()
    };

    // Build helper functions
    fn parse_nodes ( nodes_str: &&String ) -> String {
        let nodes = nodes_str.split(',').collect::<Vec<&str>>();
        let mut node_text = nodes
            .iter()
            .take(10)
            .map(|e| *e)
            .collect::<Vec<&str>>()
            .join(", ");
        if nodes.len() > 10 {
            node_text += &format!("... ({} more)", nodes.len() - 10);
        }
        node_text
    }
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

    // Build jobs and template
    let jobs = jobs.into_iter().rev().collect();
    let template = SearchPageTemplate {
        alert: if username.is_none() { 
            Some("You are not logged in!".to_string())
        } else { 
            if any_filters {
                None
            } else {
                Some("Choose your filters and click search!".to_string())
            }
        },
        username,
        title: String::from("Active Jobs - NIU"),
        header: String::from("Search"),
        jobs,

        state_query: params.get("state").and_then(|st| Some(st.to_owned())),
        queue_query: params.get("queue").and_then(|st| Some(st.to_owned())),
        user_query: params.get("user").and_then(|st| Some(st.to_owned())),
        name_query: params.get("name").and_then(|st| Some(st.to_owned())),
        date_query,

        parse_nodes,
        timestamp_to_date,
        to_i32
    };

    Ok(HtmlTemplate(template))
}