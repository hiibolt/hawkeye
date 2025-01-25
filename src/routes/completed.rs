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
#[template(path = "completed.html")]
struct CompletedPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,
    jobs: Vec<BTreeMap<String, String>>,

    user_query: Option<String>,
    date_query: Option<String>,

    div_two_i32s_into_f32: fn(&&String, &&String) -> Result<f32>,
    timestamp_to_date: fn(&&String) -> String
}
pub async fn completed(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    eprintln!("{}", "[ Got request to build completed page...]".green());

    // Unpack username and query parameters
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't get username from session! Error: {e:?}").red());
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
                eprintln!("{}", format!("Couldn't get all jobs! Error: {e:?}").red());
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get all jobs!".to_string())
            })?
    } else {
        vec!()
    };

    // Build helper functions
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
    fn div_two_i32s_into_f32 ( num1: &&String, num2: &&String ) -> Result<f32> {
        Ok(num1.parse::<f32>()
            .context("Failed to parse number 1!")?
            / num2.parse::<f32>()
                .context("Failed to parse number 2!")?)
    }

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
        title: String::from("Completed Jobs - NIU"),
        header,
        jobs,

        user_query,
        date_query,

        div_two_i32s_into_f32,
        timestamp_to_date
    };

    Ok(HtmlTemplate(template))
}