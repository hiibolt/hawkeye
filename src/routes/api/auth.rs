use axum::{
    extract::{Form, State}, http::StatusCode, response::{IntoResponse, Redirect}
};
use serde::Deserialize;
use tower_sessions::Session;
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{error, warn};

use crate::{daemons::{groups::grab_group_thread, jobs::grab_old_jobs_thread}, routes::AppState};

#[derive(Deserialize, Debug)]
pub struct LoginRequest {
    one: String, // Username (labelled as "one" to avoid autofill)
    two: String, // Password (labelled as "two" to avoid autofill)
}
#[tracing::instrument]
pub async fn login (
    State(app): State<Arc<AppState>>,
    session: Session,
    Form(payload): Form<LoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Clear the session
    session.clear().await;

    // Attempt to verify (username, password) with your "verify_login"
    let remote_username = app.remote_username.clone();
    let remote_hostname = app.remote_hostname.clone();

    let username = &payload.one;
    let password = &payload.two;

    let login_result = app
        .db
        .login(
            &remote_username,
            &remote_hostname,
            username,
            password
        )
        .await
        .map_err(|e| {
            error!(%e, "Couldn't verify login!");
            (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't verify login!".to_string())
        })?;

    if login_result.created_new {
        // Lookup groups and old jobs for the user
        let mut tasks = JoinSet::new();
        
        tasks.spawn(grab_group_thread(app.clone(), remote_username.clone(), remote_hostname.clone(), username.to_string()));
        tasks.spawn(grab_old_jobs_thread(app.clone(), remote_username, remote_hostname, username.to_string()));
        
        tasks.join_all().await;
    }

    let url_prefix = app.url_prefix.clone();
    
    match login_result.success {
        true => {
            // Store relevant info in the session (username, group(s), etc.)
            // Typically you might query the groups or store them in a single step here:
            // let groups = some_function_to_get_groups_for(&payload.username);
            
            // Now store that in the session
            session.insert("username", username).await
                .map_err(|e| {
                    error!(%e, "Couldn't insert username into session!");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't insert username into session!".to_string())
                })?;

            drop(login_result);

            Ok(Redirect::to(&(url_prefix + "/")))
        },
        false => {
            drop(login_result);

            // If not verified or an error, you can respond with an error page/JSON
            // Here we'll just return a plain text error
            warn!("[ Invalid login! ]");
            Ok(Redirect::to(&(url_prefix + "/login?invalid=true")))
        }
    }
}

pub async fn logout (
    session: Session,
) -> Result<(), (StatusCode, String)> {
    // Clear the entire session
    match session.delete().await {
        Ok(_) => {
            Ok(())
        },
        Err(_) => {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR, 
                "Failed to clear session!".to_string()
            ))
        }
    }
}
