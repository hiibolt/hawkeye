pub mod jobs;
pub mod stats;
pub mod auth;

pub struct AppState {
    pub remote_username: String,
    pub remote_hostname: String,
    pub db: super::DB
}