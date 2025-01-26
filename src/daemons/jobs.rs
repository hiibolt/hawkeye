use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use tracing::{error, info};
use tokio::task::JoinSet;
use tokio::sync::Mutex;

use crate::routes::AppState;
use super::super::{
    remote::command::*,
    parsing::jobs::*,
};

const DEFAULT_JOBSTAT_PERIOD: u64 = 60 * 5;
const DEFAULT_OLD_JOB_PERIOD: u64 = 60 * 30;

#[tracing::instrument]
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
        vec![&user, "year", "raw"],
        true
    ).await
        .context("Couldn't get output from `jmanl` command!")?;

    // Extract the job ID and # of chunks from the following:
    //  (and nothing else, the rest is garbage)
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

    let mut use_clrf = false;
    let input = if let Some(input) = old_jobs_raw.split("Raw records::\n")
        .nth(1)
    {
        input
    } else {
        info!("Couldn't use LF as delimiter! Trying CLRF...");
        use_clrf = true;
        old_jobs_raw.split("Raw records::\r\n")
            .nth(1)
            .ok_or(anyhow!("Invalid input! INput: {old_jobs_raw:?}"))?
    };
    
    let jobs = if use_clrf { 
        input.split("\r\n")
    } else {
        input.split("\n") 
    }.filter(|line| !line.is_empty())
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
                        error!("Couldn't get job ID from `jmanl` job line: {job_line}!");
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
                    error!(%e, "Couldn't parse `jmanl` job line: {job_line}!");
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
#[tracing::instrument]
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

    let mut tasks = JoinSet::new();
    for user in users {
        let app = app.clone();
        let remote_username = remote_username.clone();
        let remote_hostname = remote_hostname.clone();
        let user_cloned = user.clone();
        tasks.spawn(async move {
            if let Err(e) = grab_old_jobs_thread(
                app,
                remote_username,
                remote_hostname,
                user_cloned
            ).await {
                error!(%e, "Couldn't grab old jobs for {user}!");
            }
        });
    }

    tasks.join_all().await;

    Ok(())
}
pub async fn old_jobs_daemon (
    app: Arc<Mutex<AppState>>
) {
    let old_job_period = std::env::var("OLD_JOBS_DAEMON_PERIOD")
        .unwrap_or_else(|_| DEFAULT_OLD_JOB_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `OLD_JOBS_DAEMON_PERIOD` value!");
    info!("[ Old job period: {old_job_period} ]");

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    loop {
        info!("[ Pulling old jobs... ]");
        if let Err(e) = grab_old_jobs_helper( app.clone() ).await {
            error!(%e, "Failed to grab old jobs!");

            tokio::time::sleep(tokio::time::Duration::from_secs(
                old_job_period
            )).await;
            continue;
        };
        info!("[ Old jobs pulled! ]");

        tokio::time::sleep(tokio::time::Duration::from_secs(
            old_job_period
        )).await;
    }
}
#[tracing::instrument]
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
        .context("Failed to run remote command!")?;

    let job_strs: Vec<&str> = jobstat_output.split("--------------------\r\n")
        .nth(1)
        .with_context(|| format!("Invalid input! Input:\n{jobstat_output}"))?
        .split("\r\n\r\n")
        .collect();

    let jobs = job_strs.iter()
        .flat_map(|job| {
            if job.starts_with("nodes: ") {
                return None;
            }
            match jobstat_job_str_to_btree(job) {
                Ok(job) => Some(job),
                Err(e) => {
                    error!(%e, "Couldn't parse `jobstat` job line!");
                    error!("Job line: {job}");
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
    info!("Marking completed jobs...");
    app.lock().await
        .db
        .mark_completed_jobs(&jobs)
        .context("Couldn't mark complete jobs!")?;
    info!("Completed jobs marked!");

    Ok(())
}
#[tracing::instrument]
pub async fn jobs_daemon ( app: Arc<Mutex<AppState>> ) {
    let jobstat_period = std::env::var("JOBS_DAEMON_PERIOD")
        .unwrap_or_else(|_| DEFAULT_JOBSTAT_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `JOBS_DAEMON_PERIOD` value!");
    info!("[ Jobstat period: {jobstat_period} ]");

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    loop {
        info!("[ Pulling jobs... ]");
        if let Err(e) = grab_jobs_helper( app.clone() ).await {
            error!(%e, "Failed to run remote command!");

            tokio::time::sleep(tokio::time::Duration::from_secs(
                jobstat_period
            )).await;
            continue;
        };
        info!("[ Jobs pulled! ]");

        tokio::time::sleep(tokio::time::Duration::from_secs(
            jobstat_period
        )).await;
    }
}