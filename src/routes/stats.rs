use super::{HtmlTemplate, AppState};

use std::{collections::BTreeMap, sync::Arc};

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
#[template(path = "stats.html")]
struct StatsPageTemplate {
    username: Option<String>,
    title: String,
    header: String,
    alert: Option<String>,

    job: Option<(
        BTreeMap<String, String>,
        Vec<BTreeMap<String, String>>
    )>,
    
    div_two_i32s_into_f32: fn(&&String, &&String) -> Result<f32>,
    timestamp_to_date: fn(&&String) -> String,
    to_i32: fn(&&String) -> Result<i32>
}
pub async fn stats(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<BTreeMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    eprintln!("{}", "[ Got request to build running page...]".green());

    // Extract the username from the session 
    //  and the query parameters
    let username = session.get::<String>("username")
        .await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't get username from session! Error: {e:?}").red());
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;
    let id_query = params.get("id")
        .and_then(|st| Some(st.to_owned()));

    // Get all running jobs
    let job = if let Some(_) = username {
        if let Some(ref id) = id_query {
            let id = id.parse::<i32>()
                .map_err(|e| {
                    eprintln!("{}", format!("Failed to parse ID! Error: {e:?}").red());
                    (StatusCode::BAD_REQUEST, "Failed to parse ID!".to_string())
                })?;

            let job = app.lock()
                .await
                .db
                .get_job(id)
                .map_err(|e| {
                    eprintln!("{}", format!("Couldn't get job! Error: {e:?}").red());
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get job!".to_string())
                })?;
            let stats = app.lock()
                .await
                .db
                .get_job_stats(id)
                .map_err(|e| {
                    eprintln!("{}", format!("Couldn't get job stats! Error: {e:?}").red());
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get job stats!".to_string())
                })?;

            Some((job, stats))
        } else {
            eprintln!("{}", format!("No user query provided!").yellow());
            None
        }
    } else {
        None
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
    fn to_i32 ( num: &&String ) -> Result<i32> {
        Ok(num.parse::<f64>()
            .context("Failed to parse number!")?
            as i32)
    }
    fn div_two_i32s_into_f32 ( num1: &&String, num2: &&String ) -> Result<f32> {
        Ok(num1.parse::<f32>()
            .context("Failed to parse number 1!")?
            / num2.parse::<f32>()
                .context("Failed to parse number 2!")?)
    }

    // Build template
    let template = StatsPageTemplate {
        alert: if let Some(_) = username {
            if let None = id_query {
                Some("No job ID provided!".to_string())
            } else {
                None
            }
        } else {
            Some("You are not logged in!".to_string())
        },
        username,
        title: String::from("Job Stats - NIU"),
        header: if let Some(ref id) = id_query {
            format!("Extended Job Stats - Job ID {id}")
        } else {
            String::from("Job Stats")
        },

        job,

        div_two_i32s_into_f32,
        timestamp_to_date,
        to_i32
    };

    Ok(HtmlTemplate(template))
}