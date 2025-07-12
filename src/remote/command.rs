use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::routes::AppState;

pub async fn remote_command (
    state: &Arc<AppState>,

    command: &str,
    args: Vec<&str>,
    use_script: bool
) -> Result<String> {
    // Verify the SSH session
    state.verify_ssh_session().await
        .context("Couldn't verify SSH session!")?;

    let session = state
        .ssh_session
        .read()
        .await;
    let mut session_command = if !use_script {
        let mut session_command = session
            .command(command);
        for arg in &args {
            session_command.arg(arg);
        }
        session_command
    } else {
        let mut session_command = session
            .command("script");
        session_command.arg("-q");
        session_command.arg("-c");
        session_command.arg(format!(
            "{command} {}",
            args.join(" ")
        ));
        session_command.arg("/dev/null");

        session_command
    };

    // Run the job
    let output = session_command
        .output().await
        .context("Failed to run remote command!")?;

    // Extract the output from stdout
    let stdout = String::from_utf8(output.stdout)
        .context("Server `stdout` was not valid UTF-8")?;
    let stderr = String::from_utf8(output.stderr)
        .context("Server `stderr` was not valid UTF-8")?;

    // Treat any error output as fatal
    if !stderr.is_empty() {
        bail!("Server had `stderr`: {stderr}");
    }

    // Return as successful
    Ok(stdout)
}