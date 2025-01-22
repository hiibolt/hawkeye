use std::sync::Arc;

use anyhow::{Context, Result};
use colored::Colorize;
use futures::future::join_all;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;

use crate::routes::AppState;
use super::super::remote::command::*;

const GROUPS_PERIOD: u64 = 60 * 60;

pub async fn grab_group_thread (
    app: Arc<Mutex<AppState>>,
    remote_username: &str,
    remote_hostname: &str,
    user: &str
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

    println!("[ Got Users ]: {users:?}");

    let (remote_username, remote_hostname) = {
        let state = app.lock().await;
        
        (state.remote_username.clone(), state.remote_hostname.clone())
    };

    // Spawn a task for each user, but collect the JoinHandles
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();
    for user in users {
        let app = app.clone();
        let remote_username = remote_username.clone();
        let remote_hostname = remote_hostname.clone();
        let handle = tokio::spawn(async move {
            // We deliberately swallow the actual Result here, but you could propagate it
            if let Err(e) = grab_group_thread(
                app,
                &remote_username,
                &remote_hostname,
                &user
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

pub async fn groups_daemon (
    app: Arc<Mutex<AppState>>,
    run_once: bool
) {
    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Wait even longer for jobs to be populated
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    loop {
        eprintln!("{}", "[ Pulling groups... ]".green());
        if let Err(e) = grab_groups_helper( app.clone() ).await {
            eprintln!("[ ERROR ] Failed to run remote command! Error: {e:?}");

            tokio::time::sleep(tokio::time::Duration::from_secs(GROUPS_PERIOD)).await;
            continue;
        };
        eprintln!("{}", "[ Groups pulled! ]".green());

        if run_once {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(GROUPS_PERIOD)).await;
    }
}