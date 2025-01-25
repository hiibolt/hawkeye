use axum::response::IntoResponse;
use askama::Template;
use axum::{
    response::{Html, Response},
    http::StatusCode
};

pub mod auth;

pub mod running;
pub mod login;
pub mod completed;
pub mod search;
pub mod stats;

pub struct AppState {
    pub remote_username: String,
    pub remote_hostname: String,
    pub db: super::DB,
    pub frontend_base: String
}

struct HtmlTemplate<T>(T);
impl<T> IntoResponse for HtmlTemplate<T>
    where
        T: Template,
{
    fn into_response(self) -> Response {
        // Attempt to render the template with askama
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {}", err),
            )
                .into_response(),
        }
    }
}