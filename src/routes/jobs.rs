use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use axum::{extract::{Query, State}, response::Json};
use tokio::sync::Mutex;
use colored::Colorize;

use super::AppState;

pub async fn jobs_handler(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>
) -> Result<Json<Vec<BTreeMap<String, String>>>, String> {
    eprintln!("{}", "[ Got a request! ]".green());

    // If the user wants to filter by username, we'll do that
    if let Some(user) = params.get("user") {
        match app.lock().await
            .db
            .get_user_jobs(user)
        {
            Ok(jobs) => return Ok(Json(jobs)),
            Err(e) => {
                return Err(
                    format!("Couldn't get jobs for user {user}! Error: {e:?}")
                );
            }
        }
    }

    Err("Invalid query parameters!".to_string())
}