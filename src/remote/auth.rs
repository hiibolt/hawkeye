use std::sync::Arc;

use anyhow::{Result, Context, anyhow};
use tracing::error;

use crate::routes::AppState;

#[tracing::instrument]
pub async fn verify_login (
    state:    &Arc<AppState>,
    username: &str,
    password: &str
) -> Result<bool> {
    // Verify the SSH session
    state.verify_ssh_session().await
        .context("Couldn't verify SSH session!")?;

    // Build our command
    let session = state
        .ssh_session
        .lock()
        .await;
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