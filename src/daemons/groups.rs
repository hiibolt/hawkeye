use std::sync::Arc;

use anyhow::{Context, Result};
use colored::Colorize;
use futures::future::join_all;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;

use crate::routes::AppState;
use super::super::remote::command::*;

const GROUPS_PERIOD: u64 = 60 * 60;

async fn grab_group_thread (
    app: Arc<Mutex<AppState>>,
    user: &str,
    remote_username: String,
    remote_hostname: String
) -> Result<()> {
    let group_output: String = remote_command(
        &remote_username,
        &remote_hostname,
        "groups",
        vec![user],
        false
    ).await
        .context("[ ERROR ] Failed to run remote command!")?;

    let groups: Vec<&str> = group_output
        .split(" : ")
        .nth(1)
        .context("Invalid output from the `groups` command!")?
        .split_whitespace()
        .collect();
    eprintln!("Got groups for `{user}`: {groups:?}");

    let db = &mut app.lock().await.db;
    for group in groups {
        db.insert_user_group(user, group)
            .with_context(|| format!("Couldn't insert user {user} into group {group}!"))?;
    }
    
    eprintln!("Inserted groups for `{user}`!");

    Ok(())
}

async fn grab_groups_helper ( app: Arc<Mutex<AppState>> ) -> Result<()> {
    // Get a list of all users from the DB
    let users = app.lock().await
        .db
        .get_users()
        .context("Couldn't get users!")?;

    // Reserve the remote username and hostname
    let (remote_username, remote_hostname) = {
        let state = app.lock().await;
        
        (state.remote_username.clone(), state.remote_hostname.clone())
    };

    println!("[ Got Users ]: {users:?}");

    // Spawn a task for each user, but collect the JoinHandles
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();
    for user in users {
        let app_clone = app.clone();
        let remote_username_clone = remote_username.clone();
        let remote_hostname_clone = remote_hostname.clone();

        let handle = tokio::spawn(async move {
            // We deliberately swallow the actual Result here, but you could propagate it
            if let Err(e) = grab_group_thread(
                app_clone,
                &user,
                remote_username_clone,
                remote_hostname_clone
            ).await {
                eprintln!("[ ERROR ] Failed to grab groups for {user}! Error: {e:?}");
            }
        });

        tasks.push(handle);
    }

    // Await *all* tasks to finish
    join_all(tasks).await;

    Ok(())
}

pub async fn groups_daemon ( app: Arc<Mutex<AppState>> ) {
    loop {
        eprintln!("{}", "[ Pulling groups... ]".green());
        if let Err(e) = grab_groups_helper( app.clone() ).await {
            eprintln!("[ ERROR ] Failed to run remote command! Error: {e:?}");

            tokio::time::sleep(tokio::time::Duration::from_secs(GROUPS_PERIOD)).await;
            continue;
        };
        eprintln!("{}", "[ Groups pulled! ]".green());

        tokio::time::sleep(tokio::time::Duration::from_secs(GROUPS_PERIOD)).await;
    }
}