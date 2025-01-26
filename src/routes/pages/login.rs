use super::super::HtmlTemplate;

use std::collections::HashMap;

use anyhow::Result;
use axum::{
    extract::Query, response::IntoResponse,
    http::StatusCode
};
use tower_sessions::Session;
use askama::Template;
use tracing::info;

#[derive(Template, Debug)]
#[template(path = "pages/login.html")]
struct LoginPageTemplate {
    title: String,
    username: Option<String>,
    failed: bool
}
#[tracing::instrument]
pub async fn login(
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("[ Got request to build login page...]");

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