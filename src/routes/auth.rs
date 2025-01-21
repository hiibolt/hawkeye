use axum::{
    extract::{Form, State}, http::StatusCode, response::{IntoResponse, Redirect}
};
use serde::Deserialize;
use tower_sessions::Session;
use std::sync::Arc;
use tokio::sync::Mutex;
use colored::Colorize;

use crate::routes::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

pub async fn login_handler(
    State(app): State<Arc<Mutex<AppState>>>,
    session: Session,
    Form(payload): Form<LoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Attempt to verify (username, password) with your "verify_login"
    let (remote_username, remote_hostname) = {
        let state = app.lock().await;
        
        (state.remote_username.clone(), state.remote_hostname.clone())
    };

    let db = &mut app.lock()
        .await
        .db;
    let login_result = db
        .login(
            &remote_username,
            &remote_hostname,
            &payload.username,
            &payload.password
        )
        .await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't verify login! Error: {e:?}").red());
            (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't verify login!".to_string())
        })?;

    if login_result.created_new {
        // Launch the groups daemon early
        tokio::spawn(crate::daemons::groups::groups_daemon(app.clone()));
    }
        
    match login_result.success {
        true => {
            // Store relevant info in the session (username, group(s), etc.)
            // Typically you might query the groups or store them in a single step here:
            // let groups = some_function_to_get_groups_for(&payload.username);
            
            // Now store that in the session
            session.insert("username", &payload.username).await
                .map_err(|e| {
                    eprintln!("{}", format!("Couldn't insert username into session! Error: {e:?}").red());
                    (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't insert username into session!".to_string())
                })?;
            // session.insert("groups", groups).await.unwrap();

            // You can redirect, or return JSON, or some other response
            Ok(Redirect::to("/api/v1/jobs"))
        },
        false => {
            // If not verified or an error, you can respond with an error page/JSON
            // Here we'll just return a plain text error
            Err((StatusCode::UNAUTHORIZED, "Invalid login!".to_string()))
        }
    }
}

pub async fn logout_handler(
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Clear the entire session
    match session.delete().await {
        Ok(_) => {
            Ok(Redirect::to("/api/v1/jobs"))
        },
        Err(_) => {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR, 
                "Failed to clear session!".to_string()
            ))
        }
    }
}
