use anyhow::{Context, Result};
use openssh::{KnownHosts, Session};

pub async fn verify_login (
    remote_username: &str,
    remote_hostname: &str,
    username: &str,
    password: &str
) -> Result<bool> {
    // Attempt to connect to METIS
    let session = Session::connect_mux(&format!("{remote_username}@{remote_hostname}"), KnownHosts::Strict)
        .await
        .map_err(|e| anyhow::anyhow!("Error starting Metis connection! See below:\n{:#?}", e))?;

    // Build our command
    let mut session_command = session
        .command("/opt/metis/el8/contrib/admin/batchmon/verify_login.sh");
    session_command.arg(username);
    session_command.arg(password);

    // Check the return status of the command
    let output = session_command
        .status().await
        .context("Failed to run verify_login command!")?;

    match output.code() {
        Some(0) => Ok(true),
        _ => Ok(false)
    }
}