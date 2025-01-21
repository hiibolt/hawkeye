use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use colored::Colorize;

use crate::routes::AppState;

use super::super::{
    remote::command::*,
    parsing::jobs::*,
};
use tokio::sync::Mutex;

const JOBSTAT_PERIOD: u64 = 60 * 5;

async fn grab_jobs_helper ( app: Arc<Mutex<AppState>> ) -> Result<()> {
    let jobstat_output: String = remote_command(
        &std::env::var("REMOTE_USERNAME")
            .context("Missing `REMOTE_USERNAME` environment variable!")?,
        &std::env::var("REMOTE_HOSTNAME")
            .context("Missing `REMOTE_HOSTNAME` environment variable!")?,
        "jobstat",
        vec!["-anL"],
        true
    ).await
        .context("[ ERROR ] Failed to run remote command!")?;

    let job_strs: Vec<&str> = jobstat_output.split("----- --------- ------- ------- ----- -----------------------------------------------------------------------------------------------------------------------------\r\n")
        .nth(1)
        .with_context(|| format!("Invalid input! Input:\n{jobstat_output}"))?
        .split("\r\n\r\n")
        .collect();

    let jobs = job_strs.iter()
        .flat_map(|job| job_str_to_btree(job))
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
    loop {
        eprintln!("{}", "[ Pulling data... ]".green());
        if let Err(e) = grab_jobs_helper( app.clone() ).await {
            eprintln!("[ ERROR ] Failed to run remote command! Error: {e:?}");

            tokio::time::sleep(tokio::time::Duration::from_secs(JOBSTAT_PERIOD)).await;
            continue;
        };
        eprintln!("{}", "[ Data pulled! ]".green());

        tokio::time::sleep(tokio::time::Duration::from_secs(JOBSTAT_PERIOD)).await;
    }
}