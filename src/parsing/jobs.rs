use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, anyhow};
use chrono::{DateTime, Utc};
use tracing::{error, info};

#[tracing::instrument]
pub fn convert_mem_to_f64 ( st: &str ) -> Result<f64> {
    if let Ok(st) = st.parse() {
        return Ok(st);
    }

    if st.contains("kb") {
        return Ok(st.split("kb")
            .next()
            .context("Invalid memory string!")?
            .parse::<f64>()
            .context("Couldn't convert memory string to f64!")?
            * 0.0000009536743 );
    } else if st.contains("gb") {
        return Ok(st.split("gb")
            .next()
            .context("Invalid memory string!")?
            .parse::<f64>()
            .context("Couldn't convert memory string to f64!")?);
    } else {
        error!("Recieved unusual memory input!");
        bail!("Recieved unusual memory input!");
    }
}
#[tracing::instrument]
pub fn date_to_unix_timestamp(date_str: &str) -> Result<u32, String> {
    // Define the input date format
    let format = "%a %b %d %H:%M:%S %Y %z";

    // Parse the input string into a DateTime<Utc>
    let datetime = match DateTime::parse_from_str(
        &format!("{} {}", date_str, "+0000"),
        format
    ) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            error!(%e, "Failed to parse date!");
            return Err(format!("Failed to parse date: {}", e))
        },
    };

    // Convert the DateTime<Utc> to a UNIX timestamp
    let timestamp = datetime.timestamp();

    // Ensure the timestamp is within the range of u32
    if timestamp < 0 {
        error!("Timestamp is negative, cannot fit into u32");
        return Err("Timestamp is negative, cannot fit into u32".to_string());
    }

    Ok(timestamp as u32)
}
#[tracing::instrument]
pub fn walltime_to_percentage(reserved: &str, used: &str) -> Result<f64, String> {
    fn parse_time_to_seconds(time: &str) -> Result<u64, String> {
        let parts: Vec<&str> = time.split(':').collect();
        if parts.len() != 3 {
            error!("Invalid time format: {}", time);
            return Err(format!("Invalid time format: {}", time));
        }
        let hours: u64 = parts[0].parse().map_err(|_| format!("Invalid hours in: {}", time))?;
        let minutes: u64 = parts[1].parse().map_err(|_| format!("Invalid minutes in: {}", time))?;
        let seconds: u64 = parts[2].parse().map_err(|_| format!("Invalid seconds in: {}", time))?;
        Ok(hours * 3600 + minutes * 60 + seconds)
    }

    let reserved_seconds = parse_time_to_seconds(reserved)?;
    let used_seconds = parse_time_to_seconds(used)?;

    if reserved_seconds == 0 {
        error!("Reserved walltime cannot be zero.");
        return Err("Reserved walltime cannot be zero.".to_string());
    }

    Ok((used_seconds as f64 / reserved_seconds as f64) * 100.0)
}
#[tracing::instrument]
pub fn jmanl_job_str_to_btree<'a>(
    prelim: Vec<&'a str>,
    job: &'a str
) -> Result<BTreeMap<String, String>> {
    let mut entry = BTreeMap::new();

    info!("[ Looking at the following job ]\n{job}");

    entry.insert(
        "job_state".to_string(),
        prelim.get(1)
            .context("Invalid field!")?
            .to_string()
    );
    entry.insert(
        "job_id".to_string(),
        prelim.get(2)
            .context("Invalid field!")?
            .split('.')
            .next()
            .context("Invalid field!")?
            .to_string()
    );

    for field in job.split_ascii_whitespace() {
        let field = field.trim_ascii_start();
        let name = field.split("=")
            .next()
            .context("Invalid field!")?;
        let value = field.split("=")
            .skip(1)
            .collect::<Vec<&str>>()
            .join("=");
        info!("\t[ Got Field ]\n{name} - {value}");

        if name == "start" {
            entry.insert("start_time".to_string(), value.to_string());
            continue;
        }
        if name == "user" {
            entry.insert("Job_Owner".to_string(), value.to_string());
            continue;
        }
        if name == "jobname" {
            entry.insert("Job_Name".to_string(), value.to_string());
            continue;
        }
        entry.insert(name.to_string(), value.to_string());
    }

    info!("\t[ Converting Resources Used Memory Field... ]");
    if let Some(entry) = entry.get_mut("resources_used.mem") {
        *entry = convert_mem_to_f64(&(*entry))
            .context("Couldn't unpack memory field!")?
            .floor()
            .to_string();
    }

    info!("\t[ Converting Resource List Memory Field... ]");
    if let Some(entry) = entry.get_mut("Resource_List.mem") {
        *entry = (*entry).split("gb")
            .next()
            .context("Invalid field!")?
            .to_string();
    }

    info!("\n[ Calculating Memory Efficiency... ]");
    let mem_efficiency = 
        convert_mem_to_f64(&entry.get("resources_used.mem")
            .context("Missing field 'resources_used.mem'")?)
            .context("Couldn't unpack memory field!")?
        /
        convert_mem_to_f64(&entry.get("Resource_List.mem")
            .context("Missing field 'Resource_used.mem'")?)
            .context("Couldn't unpack memory field!")?
        * 100f64;
    entry.insert("mem_efficiency".to_string(), mem_efficiency.to_string());

    info!("\t[ Converting Resource List Nodes Field... ]");
    if let Some(exec_host_str) = entry.get_mut("exec_host") {
        let nodes = exec_host_str.split("+")
            .flat_map(|node| {
                node.split("/")
                    .next()
                    .context("Invalid field!")
            })
            .collect::<Vec<&str>>()
            .join(",");

        entry.insert("Nodes".to_string(), nodes.to_string());
    }

    info!("\t[ Adding UNIX End Timestamp... ]");
    entry.insert("end_time".to_string(), entry.get("end")
        .context("Missing field 'end'")?
        .parse::<i64>()
        .context("Couldn't parse UNIX timestamp!")?
        .to_string());

    info!("\t[ Calculating Walltime Efficiency... ]");
    let walltime_efficiency = walltime_to_percentage(
        &entry["Resource_List.walltime"],
        &entry["resources_used.walltime"]
    ).map_err(|e| anyhow!("Couldn't calculate walltime efficiency! Error: {e:?}"))?;
    entry.insert("walltime_efficiency".to_string(), walltime_efficiency.to_string());

    info!("\t[ Calculating CPU Efficiency... ]");
    let cpu_efficiency = 
    ( entry.get("resources_used.cpupercent")
        .context("Missing field 'resources_used.cpupercent'")?
        .parse::<f64>()
        .context("Couldn't parse CPU time!")? )
        /
        ( entry.get("Resource_List.ncpus")
            .context("Missing field 'Resource_List.ncpus'")?
            .parse::<f64>()
            .context("Couldn't parse CPU time!")? * 100f64 )
        * 100f64;
    entry.insert("cpu_efficiency".to_string(), cpu_efficiency.to_string());

    info!("\t\t[ Done! ]");
    Ok(entry)
}
#[tracing::instrument]
pub fn jobstat_job_str_to_btree<'a>( job: &'a str ) -> Result<BTreeMap<&'a str, String>> {
    let mut entry = BTreeMap::new();

    info!("\n[ Looking at the following job ]\n{job}");

    for (ind, field) in job.lines().enumerate() {
        if ind == 0 {
            entry.insert("job_id", field.to_string());
            info!("Inserting for Job ID from ind 0 - {field}");
            continue;
        }
        if field.starts_with("nodes: ") {
            continue;
        }

        let field = field.trim_ascii_start();
        info!("\t[ Analyzing Field ]\n{field} - {ind} - {field}");
        let name = field.split(" = ")
            .next()
            .context("Invalid field!")?;
        let value = field.split(" = ")
            .nth(1)
            .context("Invalid field!")?;

        if name == "stime" {
            // Convert the start time to a UNIX timestamp
            let timestamp = date_to_unix_timestamp(value)
                .map_err(|e| anyhow!("Couldn't convert start time to UNIX timestamp! Error: {e:?}"))?;
            entry.insert("start_time", timestamp.to_string());
            continue;
        }

        if name == "Job_Owner" {
            info!("\t[ Reformatting Job Owner... ]");
            let owner = value
                .split("@")
                .next()
                .context("Invalid field!")?
                .to_string();
            entry.insert(
                name, 
                owner
            );
            continue;
        }
        entry.insert(name, value.to_string());
    }

    if let Some(state) = entry.get("job_state") {
        if state == "Q" {
            info!("\t[ Job is in queue, inserting dummy values... ]");
            entry.insert("resources_used.mem", "0".to_string());
            entry.insert("resources_used.walltime", "00:00:00".to_string());
            entry.insert("resources_used.cpupercent", "0".to_string());
            entry.insert("start_time", i32::MAX.to_string());
            entry.insert("Nodes", "None".to_string());
        }
    } else {
        error!("Job state not found!");
        bail!("Job state not found!");
    }

    info!("\t[ Converting Resources Used Memory Field... ]");
    if let Some(entry) = entry.get_mut("resources_used.mem") {
        *entry = convert_mem_to_f64(&(*entry))
            .context("Couldn't unpack memory field!")?
            .floor()
            .to_string();
    }

    info!("\t[ Converting Resource List Memory Field... ]");
    if let Some(entry) = entry.get_mut("Resource_List.mem") {
        *entry = (*entry).split("gb")
            .next()
            .context("Invalid field!")?
            .to_string();
    }

    info!("\n[ Calculating Memory Efficiency... ]");
    let mem_efficiency = 
        convert_mem_to_f64(&entry.get("resources_used.mem")
            .unwrap_or(&String::from("0")))
            .context("Couldn't unpack memory field!")?
        /
        convert_mem_to_f64(&entry.get("Resource_List.mem")
            .context("Missing field 'Resource_used.mem'")?)
            .context("Couldn't unpack memory field!")?
        * 100f64;
    entry.insert("mem_efficiency", mem_efficiency.to_string());

    info!("\t[ Converting Resource List Nodes Field... ]");
    let walltime_efficiency = walltime_to_percentage(
        &entry.get("Resource_List.walltime").unwrap_or(&String::from("00:00:01")),
        &entry.get("resources_used.walltime").unwrap_or(&String::from("00:00:01"))
    ).map_err(|e| anyhow!("Couldn't calculate walltime efficiency! Error: {e:?}"))?;
    entry.insert("walltime_efficiency", walltime_efficiency.to_string());

    info!("\t[ Calculating CPU Efficiency... ]");
    let cpu_efficiency = 
    ( entry.get("resources_used.cpupercent")
        .context("Missing field 'resources_used.cpupercent'")?
        .parse::<f64>()
        .context("Couldn't parse CPU time!")? )
        /
        ( entry.get("Resource_List.ncpus")
            .context("Missing field 'Resource_List.ncpus'")?
            .parse::<f64>()
            .context("Couldn't parse CPU time!")? * 100f64 )
        * 100f64;
    entry.insert("cpu_efficiency", cpu_efficiency.to_string());

    Ok(entry)
}