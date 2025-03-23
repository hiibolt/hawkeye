use crate::routes::AppState;

use super::try_render_template;

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode, response::Response
};
use tower_sessions::Session;
use askama::Template;
use tracing::info;

#[derive(Template, Debug)]
#[template(path = "pages/login.html")]
struct LoginPageTemplate<'a> {
    title: String,
    username: Option<String>,
    failed: bool,
    url_prefix: &'a str
}
#[tracing::instrument]
pub async fn login(
    State(app): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    session: Session,
) -> Result<Response, (StatusCode, String)> {
    info!("[ Got request to build login page...]");

    let url_prefix = &app.url_prefix;

    let template = LoginPageTemplate {
        title: "Login - CRCD Batchmon".to_string(),
        username: None,
        failed: params.get("invalid")
            .and_then(|st| {
                Some(st.parse::<bool>()
                    .unwrap_or(false))
            })
            .unwrap_or(false),
        url_prefix
    };

    try_render_template(&template)
}