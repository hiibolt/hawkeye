use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use colored::Colorize;
use futures::future::join_all;
use regex::Regex;

use crate::routes::AppState;

use super::super::{
    remote::command::*,
    parsing::jobs::*,
};
use tokio::sync::Mutex;

const DEFAULT_JOBSTAT_PERIOD: u64 = 60 * 5;
const DEFAULT_OLD_JOB_PERIOD: u64 = 60 * 30;

pub async fn grab_old_jobs_thread (
    app: Arc<Mutex<AppState>>,
    remote_username: String,
    remote_hostname: String,
    user: String
) -> Result<()> {
    let old_jobs_raw = remote_command(
        &remote_username,
        &remote_hostname,
        "jmanl",
        vec![&user, "month", "raw"],
        true
    ).await
        .context("Couldn't get output from `jmanl` command!")?;

    // Extract the job ID and # of chunks from the following:
    //  (and nothing else, the rest is garbage)
    // 'Job 31940.cm-z1784300-mdmech15 (180 CPUs, 15 node(s), 15 chunk(s))'
    let formatted_jmantl_re = Regex::new(r"Job (\d+)\.cm-.+-.+ \(\d+ CPUs, \d+ node\(s\), (\d+) chunk\(s\)\)")
        .context("Couldn't compile regex!")?;

    // Create a BTreeMap from the job line
    let mut chunks_map = BTreeMap::new();
    for (_, [job_id, num_chunks]) in formatted_jmantl_re
        .captures_iter(&old_jobs_raw)
        .map(|c| c.extract())
    {
        chunks_map.insert(job_id, num_chunks);
    }

    let input = old_jobs_raw.split("Raw records::\r\n")
        .nth(1)
        .context("Invalid input!")?;

    let jobs = input.split("\r\n")
        .filter(|line| !line.is_empty())
        .flat_map(|job_line| {
            match jmanl_job_str_to_btree(
                job_line.split(';')
                        .take(3)
                        .collect::<Vec<&str>>(),
                &job_line.split(';')
                        .skip(3)
                        .collect::<Vec<&str>>()
                        .join(";")
            ) {
                Ok(job) => {
                    let job_id = if let Some(job_id) = job.get("job_id") {
                        job_id
                    } else {
                        eprintln!("{}", format!("Couldn't get job ID from `jmanl` job line: {job_line}!").red());
                        return None;
                    };

                    let num_chunks = chunks_map.get((*job_id).as_str())
                        .unwrap_or(&"?");

                    let mut job = job.clone();
                    job.insert(
                        "chunks".to_string(),
                        num_chunks.to_string()
                    );

                    Some(job)
                },
                Err(e) => {
                    eprintln!("{}", format!("Couldn't parse `jmanl` job line: {job_line}! Error: {e:?}").red());
                    None
                }
            }
        })
        .collect::<Vec<BTreeMap<String, String>>>();

    // Because the job comes out as a BTreeMap<String, String>,
    //  we need to convert it to a BTreeMap<&str, String>
    let jobs = jobs.iter()
        .map(|job| {
            job.iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect::<BTreeMap<&str, String>>()
        })
        .collect::<Vec<BTreeMap<&str, String>>>();
    
    for job in jobs.iter() {
        app.lock().await
            .db
            .insert_job(job)
            .with_context(|| format!("Couldn't insert job {job:?}!"))?;
    }

    Ok(())
}
async fn grab_old_jobs_helper (
    app: Arc<Mutex<AppState>>
) -> Result<()> {
    // Get a list of all users from the DB
    let users = app.lock().await
        .db
        .get_users()
        .context("Couldn't get users!")?;

    let (remote_username, remote_hostname) = {
        let state = app.lock().await;
        
        (state.remote_username.clone(), state.remote_hostname.clone())
    };

    let mut handles = vec![];
    for user in users {
        let app = app.clone();
        let remote_username = remote_username.clone();
        let remote_hostname = remote_hostname.clone();
        let user_cloned = user.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = grab_old_jobs_thread(
                app,
                remote_username,
                remote_hostname,
                user_cloned
            ).await {
                eprintln!("{}", format!("Couldn't grab old jobs for {user}! Error: {e:?}").red());
            }
        });

        handles.push(handle);
    }

    join_all(handles).await;

    Ok(())
}
pub async fn old_jobs_daemon (
    app: Arc<Mutex<AppState>>
) {
    let old_job_period = std::env::var("OLD_JOBS_DAEMON_PERIOD")
        .unwrap_or_else(|_| DEFAULT_OLD_JOB_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `OLD_JOBS_DAEMON_PERIOD` value!");
    eprintln!("{}", format!("[ Old job period: {old_job_period} ]").blue());

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    loop {
        eprintln!("{}", "[ Pulling old jobs... ]".green());
        if let Err(e) = grab_old_jobs_helper( app.clone() ).await {
            eprintln!("{}", format!("[ ERROR ] Failed to grab old jobs! Error: {e:?}").red());

            tokio::time::sleep(tokio::time::Duration::from_secs(
                old_job_period
            )).await;
            continue;
        };
        eprintln!("{}", "[ Old jobs pulled! ]".green());

        tokio::time::sleep(tokio::time::Duration::from_secs(
            old_job_period
        )).await;
    }
}

async fn grab_jobs_helper ( app: Arc<Mutex<AppState>> ) -> Result<()> {
    let (username, hostname) = {
        let state = app.lock().await;
        
        (state.remote_username.clone(), state.remote_hostname.clone())
    };

    let jobstat_output: String = remote_command(
        &username,
        &hostname,
        "jobstat",
        vec!["-anL"],
        true
    ).await
        .context("[ ERROR ] Failed to run remote command!")?;

    let job_strs: Vec<&str> = jobstat_output.split("--------------------\r\n")
        .nth(1)
        .with_context(|| format!("Invalid input! Input:\n{jobstat_output}"))?
        .split("\r\n\r\n")
        .collect();

    let jobs = job_strs.iter()
        .flat_map(|job| {
            match jobstat_job_str_to_btree(job) {
                Ok(job) => Some(job),
                Err(e) => {
                    eprintln!("{}", format!("Couldn't parse `jobstat` job line: {job}! Error: {e:?}").red());
                    None
                }
            }
        })
        .collect::<Vec<BTreeMap<&str, String>>>();

    for job in jobs.iter() {
        app.lock().await
            .db
            .insert_job(job)
            .with_context(|| "Couldn't insert job {job:?}!")?;
    }

    // Mark jobs that are no longer active as 'S' (stopped)
    eprintln!("{}", "[ Marking completed jobs... ]".green());
    app.lock().await
        .db
        .mark_completed_jobs(&jobs)
        .context("Couldn't mark complete jobs!")?;
    eprintln!("{}", "[ Completed jobs marked! ]".green());

    Ok(())
}

pub async fn jobs_daemon ( app: Arc<Mutex<AppState>> ) {
    let jobstat_period = std::env::var("JOBS_DAEMON_PERIOD")
        .unwrap_or_else(|_| DEFAULT_JOBSTAT_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `JOBS_DAEMON_PERIOD` value!");
    eprintln!("{}", format!("[ Jobstat period: {jobstat_period} ]").blue());

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    loop {
        eprintln!("{}", "[ Pulling jobs... ]".green());
        if let Err(e) = grab_jobs_helper( app.clone() ).await {
            eprintln!("[ ERROR ] Failed to run remote command! Error: {e:?}");

            tokio::time::sleep(tokio::time::Duration::from_secs(
                jobstat_period
            )).await;
            continue;
        };
        eprintln!("{}", "[ Jobs pulled! ]".green());

        tokio::time::sleep(tokio::time::Duration::from_secs(
            jobstat_period
        )).await;
    }
}