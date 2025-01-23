use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use axum::{extract::{Query, State}, http::StatusCode, response::Json};
use tokio::sync::Mutex;
use colored::Colorize;
use tower_sessions::Session;

use super::AppState;

pub async fn jobs_handler(
    State(app): State<Arc<Mutex<AppState>>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<Json<Vec<BTreeMap<String, String>>>, (StatusCode, String)> {
    eprintln!("{}", "[ Got a `jobs` request! ]".green());

    // Grab filter parameters
    eprintln!("Parameters: {params:?}");

    let filter_state = params.get("state");
    let filter_queue = params.get("queue");
    let filter_owner = params.get("owner");
    let filter_name = params.get("name");

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
        return match app.lock().await
            .db
            .get_all_running_jobs(
                None,
                filter_queue,
                None,
                filter_name,
                true
            )
        {
            Ok(jobs) => {
                if params.get("user").is_some() {
                    eprintln!("\t{}", "[ Not authorized to see someone else's jobs without logging in! ]".red());
                    Err((
                        StatusCode::UNAUTHORIZED,
                        "Not authorized to see someone else's jobs without logging in!".to_string()
                    ))
                } else if params.get("group").is_some() {
                    eprintln!("\t{}", "[ Not authorized to see group jobs without logging in! ]".red());
                    Err((
                        StatusCode::UNAUTHORIZED,
                        "Not authorized to see group jobs without logging in!".to_string()
                    ))
                } else {
                    eprintln!("\t{}", "[ Showing all jobs! ]".green());
                    Ok(Json(jobs))
                }
            },
            Err(e) => {
                eprintln!("{}", format!("Couldn't get all jobs! Error: {e:?}").red());
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Couldn't get all jobs! Error: {e:?}")
                ))
            }
        };
    };

    let is_admin = app.lock().await
        .db
        .is_user_admin(&username)
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't check if user is admin! Error: {e:?}").red());
            (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't check if user is admin!".to_string())
        })?;

    // If the user wants to filter by username, we'll do that
    if let Some(user_param) = params.get("user") {
        if user_param == &username || is_admin {
            match app.lock().await
                .db
                .get_user_jobs(
                    user_param,
                    filter_state,
                    filter_queue,
                    filter_owner,
                    filter_name
                )
            {
                Ok(jobs) => return Ok(Json(jobs)),
                Err(e) => return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Couldn't get jobs for user {user_param}! Error: {e:?}")
                ))
            }
        } else {
            // Or if you want to enforce group membership, do that check here
            // Or if not authorized, return an error.
            return Err((
                StatusCode::UNAUTHORIZED,
                "Not authorized to see someone else's jobs!".to_string()
            ));
        }
    }

    // If the user wants to filter by group, we'll do that
    if let Some(group) = params.get("group") {
        // Check that the user is in the group
        if !is_admin && !app.lock().await
            .db
            .is_user_in_group(&username, group)
            .map_err(|e| {
                eprintln!("{}", format!("Couldn't check if user is in group! Error: {e:?}").red());
                (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't check if user is in group!".to_string())
            })?
        {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Not authorized to see jobs in this group!".to_string()
            ));
        }

        match app.lock().await
            .db
            .get_group_jobs(
                filter_state,
                filter_queue,
                filter_owner,
                filter_name,
                group
            )
        {
            Ok(jobs) => return Ok(Json(jobs)),
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Couldn't get jobs for group {group}! Error: {e:?}")
                ));
            }
        }
    }

    // Lastly, if the user doesn't provide any query parameters, 
    // we'll just return all running jobs, but censor job owners
    match app.lock().await
        .db
        .get_all_running_jobs(
            filter_state,
            filter_queue,
            if is_admin { filter_owner } else { None },
            filter_name,
            !is_admin
        )
    {
        Ok(jobs) => return Ok(Json(jobs)),
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Couldn't get all jobs! Error: {e:?}")
            ));
        }
    }
}