use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, anyhow};
use colored::Colorize;

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
        bail!("Recieved unusual memory input!");
    }
}

pub fn walltime_to_percentage(reserved: &str, used: &str) -> Result<f64, String> {
    fn parse_time_to_seconds(time: &str) -> Result<u64, String> {
        let parts: Vec<&str> = time.split(':').collect();
        if parts.len() != 3 {
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
        return Err("Reserved walltime cannot be zero.".to_string());
    }

    Ok((used_seconds as f64 / reserved_seconds as f64) * 100.0)
}

pub fn job_str_to_btree<'a>( job: &'a str ) -> Result<BTreeMap<&'a str, String>> {
    let mut entry = BTreeMap::new();

    eprintln!("\n{}\n{job}", "[ Looking at the following job ]".green());

    for (ind, field) in job.lines().enumerate() {
        if ind == 0 {
            entry.insert("job_id", field.to_string());
            eprintln!("label - {ind} - {field}");
            continue;
        }

        let field = field.trim_ascii_start();
        eprintln!("\t{}\n{field} - {ind} - {field}", "[ Analyzing Field ]".green());
        let name = field.split(" = ")
            .next()
            .context("Invalid field!")?;
        let value = field.split(" = ")
            .nth(1)
            .context("Invalid field!")?;

        if name == "Job_Owner" {
            eprintln!("\t{}", "[ Reformatting Job Owner... ]".blue());
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

    eprintln!("\t{}", "[ Converting Resources Used Memory Field... ]".blue());
    if let Some(entry) = entry.get_mut("resources_used.mem") {
        *entry = convert_mem_to_f64(&(*entry))
            .context("Couldn't unpack memory field!")?
            .floor()
            .to_string();
    }
    eprintln!("\t{}", "[ Converting Resource List Memory Field... ]".blue());
    if let Some(entry) = entry.get_mut("Resource_List.mem") {
        *entry = (*entry).split("gb")
            .next()
            .context("Invalid field!")?
            .to_string();
    }
    eprintln!("\t{}", "[ Calculating Memory Efficiency... ]".blue());
    let mem_efficiency = 
        convert_mem_to_f64(&entry.get("resources_used.mem")
            .context("Missing field 'resources_used.mem'")?)
            .context("Couldn't unpack memory field!")?
        /
        convert_mem_to_f64(&entry.get("Resource_List.mem")
            .context("Missing field 'Resource_used.mem'")?)
            .context("Couldn't unpack memory field!")?
        * 100f64;
    entry.insert("mem_efficiency", mem_efficiency.to_string());

    eprintln!("\t{}", "[ Calculating Walltime Efficiency... ]".blue());
    let walltime_efficiency = walltime_to_percentage(
        &entry["Resource_List.walltime"],
        &entry["resources_used.walltime"]
    ).map_err(|e| anyhow!("Couldn't calculate walltime efficiency! Error: {e:?}"))?;
    entry.insert("walltime_efficiency", walltime_efficiency.to_string());

    Ok(entry)
}