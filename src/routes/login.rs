use super::HtmlTemplate;

use std::collections::HashMap;

use anyhow::Result;
use axum::{
    extract::Query, response::IntoResponse
};
use colored::Colorize;
use tower_sessions::Session;
use http::StatusCode;
use askama::Template;


#[derive(Template)]
#[template(path = "login.html")]
struct LoginPageTemplate {
    title: String,
    username: Option<String>,
    failed: bool
}
pub async fn login(
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    eprintln!("{}", "[ Got request to build login page...]".green());

    // Destroy the session
    session.delete().await
        .map_err(|e| {
            eprintln!("{}", format!("Couldn't clear session! Error: {e:?}").red());
            (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't clear session!".to_string())
        })?;

    let template = LoginPageTemplate {
        title: "Login - CRCD Batchmon".to_string(),
        username: None,
        failed: params.get("invalid")
            .and_then(|st| {
                Some(st.parse::<bool>()
                    .unwrap_or(false))
            })
            .unwrap_or(false)
    };

    Ok(HtmlTemplate(template))
}