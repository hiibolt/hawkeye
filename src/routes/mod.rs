use axum::response::IntoResponse;
use askama::Template;
use axum::{
    response::{Html, Response},
    http::StatusCode
};

use anyhow::Result;
use tokio::io::AsyncReadExt;
use axum::http::header;


pub mod api;
pub mod pages;

#[derive(Debug, Clone, Copy)]
pub struct ClusterStatus {
    pub total_nodes: u32,
    pub used_nodes:  u32,
    pub total_cpus:  u32,
    pub used_cpus:   u32,
    pub total_gpus:  u32,
    pub used_gpus:   u32,
}

#[derive(Debug)]
pub struct AppState {
    pub remote_username: String,
    pub remote_hostname: String,
    pub db: super::DB,

    pub status: Option<ClusterStatus>
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

pub async fn get_favicon ( ) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut file = match tokio::fs::File::open("public/images/favicon.ico").await {
        Ok(file) => file,
        Err(err) => return Err((StatusCode::NOT_FOUND, format!("File not found: {}", err))),
    };
    
    // Read the file into a byte array
    let mut contents: Vec<u8> = Vec::new();
    file.read_to_end(&mut contents).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read file: {}", e)
        ))?;

    let headers = [
        (header::CONTENT_TYPE, "image/x-icon"),
        (
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"favicon.ico\""
        ),
    ];

    Ok((headers, contents))
}