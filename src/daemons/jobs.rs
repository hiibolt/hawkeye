use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use tracing::{error, info};
use tokio::task::JoinSet;

use crate::routes::AppState;
use super::super::{
    remote::command::*,
    parsing::jobs::*,
};

const DEFAULT_JOBSTAT_PERIOD: u64 = 60 * 5;
const DEFAULT_OLD_JOB_PERIOD: u64 = 60 * 30;

fn render_full_error (
    e: &anyhow::Error
) -> String {
    let mut full_error = String::new();
    full_error.push_str(&format!("Error: {}\n", e));
    full_error.push_str("Full error chain:\n");
    for (i, cause) in e.chain().enumerate() {
        full_error.push_str(&format!("  {}: {}\n", i, cause));
    }

    full_error
}

#[tracing::instrument]
pub async fn grab_old_jobs_thread (
    app: Arc<AppState>,
    remote_username: String,
    remote_hostname: String,
    user: String
) -> Result<()> {
    let old_jobs_raw = remote_command(
        &remote_username,
        &remote_hostname,
        "jmanl",
        vec!(&user, "year", "raw"),
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
            .ok_or(anyhow!("Invalid input! Input: {old_jobs_raw:?}"))?
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
        app.db
            .insert_job(job)
            .await
            .with_context(|| format!("Couldn't insert old job {job:?}!"))?;
    }

    Ok(())
}
#[tracing::instrument]
async fn grab_old_jobs_helper (
    app: Arc<AppState>
) -> Result<()> {
    // Get a list of all users from the DB
    let users = app
        .db
        .get_users()
        .await
        .context("Couldn't get users!")?;

    let remote_username = app.remote_username.clone();
    let remote_hostname = app.remote_hostname.clone();

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
                let full_error = render_full_error(&e);
                error!("Couldn't grab old jobs for {user}! {full_error}");
            }
        });
    }

    tasks.join_all().await;

    Ok(())
}
pub async fn old_jobs_daemon (
    app: Arc<AppState>
) -> ! {
    let old_job_period = std::env::var("OLD_JOBS_DAEMON_PERIOD")
        .unwrap_or_else(|_| DEFAULT_OLD_JOB_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `OLD_JOBS_DAEMON_PERIOD` value!");
    info!("Old job period: {old_job_period}");

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    loop {
        info!("Pulling old jobs...");
        if let Err(e) = grab_old_jobs_helper( app.clone() ).await {
            error!(%e, "Failed to grab old jobs!");

            tokio::time::sleep(tokio::time::Duration::from_secs(
                old_job_period
            )).await;
            continue;
        };
        info!("Old jobs pulled!");

        tokio::time::sleep(tokio::time::Duration::from_secs(
            old_job_period
        )).await;
    }
}
#[tracing::instrument]
async fn grab_jobs_helper (
    app: Arc<AppState>
) -> Result<()> {
    let username = app.remote_username.clone();
    let hostname = app.remote_hostname.clone();

    let jobstat_output: String = remote_command(
        &username,
        &hostname,
        "jobstat",
        vec!("-anL"),
        true
    ).await
        .context("Failed to run remote command!")?
        .replace("\r", "");

    let cluster_status_data_raw = jobstat_output.split("nodes: ")
        .nth(1)
        .ok_or(anyhow!("Invalid cluster status input! Input:\n{jobstat_output:?}"))?
        .replace("CPU cores: ", "")
        .replace("GPU cores: ", "")
        .replace("used + ", "")
        .replace("unused + ", "")
        .replace("unavailable = ", "")
        .replace(" total", "")
        .replace("status: [R]unning", "")
        .replace("[Q]ueued", "");
    let node_stats = cluster_status_data_raw.split("\n")
        .next()
        .context("Invalid cluster status (nodes) input! Input:\n{cluster_status_data_raw:?}")?
        .split(" ")
        .collect::<Vec<&str>>();
    let cpu_stats = cluster_status_data_raw.split("\n")
        .nth(1)
        .context("Invalid cluster status (CPUs) input! Input:\n{cluster_status_data_raw:?}")?
        .split(" ")
        .collect::<Vec<&str>>();
    let gpu_stats = cluster_status_data_raw.split("\n")
        .nth(2)
        .context("Invalid cluster status (GPUs) input! Input:\n{cluster_status_data_raw:?}")?
        .split(" ")
        .collect::<Vec<&str>>();
    
    *app.status.write().await = Some(crate::routes::ClusterStatus {
        total_nodes: node_stats.get(node_stats.len() - 1).context("Missing node field 3")?.parse::<u32>()?,
        used_nodes: node_stats.get(0).context("Missing node field 0")?.parse::<u32>()?,
        total_cpus: cpu_stats.get(cpu_stats.len() - 1).context("Missing cpu field 3")?.parse::<u32>()?,
        used_cpus: cpu_stats.get(0).context("Missing cpu field 0")?.parse::<u32>()?,
        total_gpus: gpu_stats.get(gpu_stats.len() - 1).context("Missing gpu field 3")?.parse::<u32>()?,
        used_gpus: gpu_stats.get(0).context("Missing gpu field 0")?.parse::<u32>()?,
    });

    let job_strs: Vec<&str> = jobstat_output.split("--------------------\n")
        .nth(1)
        .with_context(|| format!("Invalid input! Input:\n{jobstat_output}"))?
        .split("\n\n")
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
        app.db
            .insert_job(job)
            .await
            .with_context(|| "Couldn't insert new job {job:?}!")?;
    }

    // Mark jobs that are no longer active as 'S' (stopped)
    info!("Marking completed jobs...");
    if let Err(e) = app.db
        .mark_completed_jobs(&jobs)
        .await
        .context("Couldn't mark complete jobs!")
    {
        let full_error = render_full_error(&e);
        error!("Couldn't mark completed jobs! {full_error}");
    } else {
        info!("Completed jobs marked successfully!");
    }

    Ok(())
}
pub async fn jobs_daemon ( app: Arc<AppState> ) -> ! {
    let jobstat_period = std::env::var("JOBS_DAEMON_PERIOD")
        .unwrap_or_else(|_| DEFAULT_JOBSTAT_PERIOD.to_string())
        .parse::<u64>()
        .expect("Invalid `JOBS_DAEMON_PERIOD` value!");
    info!("Jobstat period: {jobstat_period}");

    // Wait for the web server to start up
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    loop {
        info!("Pulling jobs...");
        if let Err(e) = grab_jobs_helper( app.clone() ).await {
            error!(%e, "Failed to run remote command!");

            tokio::time::sleep(tokio::time::Duration::from_secs(
                jobstat_period
            )).await;
            continue;
        };
        info!("Jobs pulled!");

        tokio::time::sleep(tokio::time::Duration::from_secs(
            jobstat_period
        )).await;
    }
}
