use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use axum::{extract::{Query, State}, http::StatusCode, response::Json};
use tokio::sync::Mutex;
use colored::Colorize;
use tower_sessions::Session;

use super::AppState;

pub async fn stats_handler(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session
) -> Result<Json<Vec<BTreeMap<String, String>>>, (StatusCode, String)> {
    eprintln!("{}", "[ Got a request to `stats`! ]".green());

    // Check if the user is in the session
    let maybe_username = session.get::<String>("username")
        .await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't get username from session! Error: {e:?}").red());
            (StatusCode::UNAUTHORIZED, "Couldn't get username from session!".to_string())
        })?;

    // If there's no "username", the user is not logged in => only show censored jobs
    let username = if let Some(username) = maybe_username {
        eprintln!("\t{}", "[ User is logged in! ]".green());
        username
    } else {
        // The user is not logged in
        eprintln!("\t{}", "[ User not logged in! ]".yellow());
        return Err((
            StatusCode::UNAUTHORIZED,
            "Not authorized to see stats without logging in!".to_string()
        ));
    };

    // If the user wants to filter by ID, we'll do that
    if let Some(id) = params.get("id") {
        let id = id.parse::<i32>()
            .map_err(|_| (
                StatusCode::BAD_REQUEST,
                "Invalid ID!".to_string()
            ))?;

        // Check that the user or one of the user's groups
        // is authorized to see the stats
        if app.lock().await
            .db
            .is_user_able_to_view_stats(&username, id)
            .map_err(|e| {
                eprintln!("{}", format!("Couldn't check if user {username} can view stats for job {id}! Error: {e:?}").red());
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't check if user can view stats!".to_string())
            })?
        {
            match app.lock().await
                .db
                .get_job_stats(id)
            {
                Ok(stats) => {
                    eprintln!("{} {:#?}", "[ Final Stats ]:".yellow(), stats);
                    return Ok(Json(stats))
                },
                Err(e) => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Couldn't get stats for user {id}! Error: {e:?}")
                    ));
                }
            }
        } else {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Not authorized to see stats for this job!".to_string()
            ));
        }
    }

    Err((
        StatusCode::BAD_REQUEST,
        "Invalid query parameters!".to_string()
    ))
}