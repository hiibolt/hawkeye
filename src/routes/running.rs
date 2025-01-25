use super::{HtmlTemplate, AppState};

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use axum::{
    extract::State, response::IntoResponse
};
use colored::Colorize;
use tower_sessions::Session;
use http::StatusCode;
use askama::Template;


#[derive(Template)]
#[template(path = "running.html")]
struct RunningPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,
    parse_nodes: fn(&&String) -> String,
    timestamp_to_date: fn(&&String) -> String,
    to_i32: fn(&&String) -> Result<i32>
}
pub async fn running(
    State(app): State<Arc<Mutex<AppState>>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    eprintln!("{}", "[ Got request to build running page...]".green());

    // Extract the username from the session
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't get username from session! Error: {e:?}").red());
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;

    // Get all running jobs
    let jobs = if let Some(_) = username {
        app.lock()
            .await
            .db
            .get_all_running_jobs(
                Some(&"R".to_string()),
                None,
                None,
                None,
                None,
                false
            )
            .map_err(|e| {
                eprintln!("{}", format!("Couldn't get all jobs! Error: {e:?}").red());
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
    } else {
        app.lock()
            .await
            .db
            .get_all_running_jobs(
                Some(&"R".to_string()),
                None,
                None,
                None,
                None,
                true
            )
            .map_err(|e| {
                eprintln!("{}", format!("Couldn't get all jobs! Error: {e:?}").red());
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
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
    let template = RunningPageTemplate {
        username,
        title: String::from("Active Jobs - NIU"),
        header: String::from("All Running Jobs on Metis"),
        alert: None,
        jobs,

        parse_nodes,
        timestamp_to_date,
        to_i32
    };

    Ok(HtmlTemplate(template))
}