use anyhow::{Result, anyhow};
use openssh::{KnownHosts, Session};
use tracing::error;

#[tracing::instrument]
pub async fn verify_login (
    remote_username: &str,
    remote_hostname: &str,
    username: &str,
    password: &str
) -> Result<bool> {
    // Attempt to connect to METIS
    let session = Session::connect_mux(&format!("{remote_username}@{remote_hostname}"), KnownHosts::Strict)
        .await
        .map_err(|e| {
            error!(%e, "Error starting Metis connection!");
            anyhow!("Error starting Metis connection! See below:\n{:#?}", e)
        })?;

    // Build our command
    let mut session_command = session
        .command("/opt/metis/el8/contrib/admin/batchmon/verify_login.sh");
    session_command.arg(username);
    session_command.arg(password);

    // Check the return status of the command, and
    //  throw out both stdout and stderr
    let output = session_command
        .stdout(openssh::Stdio::null())
        .stderr(openssh::Stdio::null())
        .status().await
        .map_err(|e| {
            error!(%e, "Failed to run verify_login command!");
            anyhow!("Failed to run verify_login command! Error: {e:?}")
        })?;

    match output.code() {
        Some(0) => Ok(true),
        _ => Ok(false)
    }
}