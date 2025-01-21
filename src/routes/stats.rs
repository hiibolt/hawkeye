use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use axum::{extract::{Query, State}, response::Json};
use tokio::sync::Mutex;
use colored::Colorize;

use super::AppState;

pub async fn stats_handler(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>
) -> Result<Json<Vec<BTreeMap<String, String>>>, String> {
    eprintln!("{}", "[ Got a request! ]".green());

    // If the user wants to filter by username, we'll do that
    if let Some(id) = params.get("id") {
        let id = id.parse::<i32>()
            .map_err(|_| "Invalid ID!")?;

        match app.lock().await
            .db
            .get_job_stats(id)
        {
            Ok(stats) => return Ok(Json(stats)),
            Err(e) => {
                return Err(
                    format!("Couldn't get stats for user {id}! Error: {e:?}")
                );
            }
        }
    }

    Err("Invalid query parameters!".to_string())
}