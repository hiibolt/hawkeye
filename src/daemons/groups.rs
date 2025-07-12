use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{info, error};

use crate::{daemons::jobs::render_full_error, routes::AppState};
use super::super::remote::command::*;

const GROUPS_PERIOD: u64 = 60 * 60;

#[tracing::instrument]
pub async fn grab_group_thread (
    app: Arc<AppState>,
    user: String
) -> Result<()> {
    let group_output: String = remote_command(
        &app,
        "groups",
        vec![&user],
        false
    ).await
        .context("Failed to run remote command!")?;

    let groups: Vec<&str> = group_output
        .split(" : ")
        .nth(1)
        .context("Invalid output from the `groups` command!")?
        .split_whitespace()
        .collect();
    info!("Got groups for `{user}`: {groups:?}");

    app.db
        .insert_user_groups(&user, groups)
        .await
        .context("Couldn't insert user groups!")?;
    
    info!("Inserted groups for `{user}`!");

    Ok(())
}
#[tracing::instrument]
async fn grab_groups_helper ( app: Arc<AppState> ) -> Result<()> {
    // Get a list of all users from the DB
    let users = app.db
        .get_users()
        .await
        .context("Couldn't get users!")?;

    info!("[ Got Users ]: {users:?}");

    // Spawn a task for each user, but collect the JoinHandles
    for user in users {
        let app = app.clone();
        let user_cloned = user.clone();

        // We deliberately swallow the actual Result here, but you could propagate it
        if let Err(e) = grab_group_thread(
            app,
            user_cloned
        ).await {
            let e = render_full_error(&e);
            error!(%e, "Failed to grab groups for {user}!");
        }
    }

    Ok(())
}
pub async fn groups_daemon (
    app: Arc<AppState>
) -> ! {
    let groups_period = std::env::var("GROUPS_DAEMON_PERIOD")
        .unwrap_or(GROUPS_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `GROUPS_DAEMON_PERIOD` value!");
    info!("[ Groups period: {groups_period} ]");

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Wait even longer for jobs to be populated
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    loop {
        info!("[ Pulling groups... ]");
        if let Err(e) = grab_groups_helper( app.clone() ).await {
            error!(%e, "Failed to run remote command!");

            tokio::time::sleep(tokio::time::Duration::from_secs(groups_period)).await;
            continue;
        };
        info!("[ Groups pulled! ]");

        tokio::time::sleep(tokio::time::Duration::from_secs(
            groups_period
        )).await;
    }
}