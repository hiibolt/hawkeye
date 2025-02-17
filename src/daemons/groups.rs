use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::task::JoinSet;
use tokio::sync::Mutex;
use tracing::{info, error};

use crate::routes::AppState;
use super::super::remote::command::*;

const GROUPS_PERIOD: u64 = 60 * 60;

#[tracing::instrument]
pub async fn grab_group_thread (
    app: Arc<Mutex<AppState>>,
    remote_username: String,
    remote_hostname: String,
    user: String
) -> Result<()> {
    let group_output: String = remote_command(
        &remote_username,
        &remote_hostname,
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

    let db = &mut app.lock().await.db;
    for group in groups {
        db.insert_user_group(&user, group)
            .with_context(|| format!("Couldn't insert user {user} into group {group}!"))?;
    }
    
    info!("Inserted groups for `{user}`!");

    Ok(())
}
#[tracing::instrument]
async fn grab_groups_helper ( app: Arc<Mutex<AppState>> ) -> Result<()> {
    // Get a list of all users from the DB
    let users = app.lock().await
        .db
        .get_users()
        .context("Couldn't get users!")?;

    info!("[ Got Users ]: {users:?}");

    let (remote_username, remote_hostname) = {
        let state = app.lock().await;
        
        (state.remote_username.clone(), state.remote_hostname.clone())
    };

    // Spawn a task for each user, but collect the JoinHandles
    let mut tasks = JoinSet::new();
    for user in users {
        let app = app.clone();
        let remote_username = remote_username.clone();
        let remote_hostname = remote_hostname.clone();
        let user_cloned = user.clone();
        tasks.spawn(async move {
            // We deliberately swallow the actual Result here, but you could propagate it
            if let Err(e) = grab_group_thread(
                app,
                remote_username,
                remote_hostname,
                user_cloned
            ).await {
                error!(%e, "Failed to grab groups for {user}!");
            }
        });
    }

    // Await *all* tasks to finish
    tasks.join_all().await;

    Ok(())
}
pub async fn groups_daemon (
    app: Arc<Mutex<AppState>>
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