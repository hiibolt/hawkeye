use std::collections::BTreeMap;
use tracing::info;
use anyhow::{Context, Result};

pub mod running;
pub mod login;
pub mod completed;
pub mod search;
pub mod stats;
pub mod queued;

#[derive(Debug)]
struct TableEntry {
    name: String,
    sort_by: String,
    value: String,
    value_unit: String,
    colored: bool,
}

// Field helper functions
fn shorten_name_field ( name_field: &mut String ) {
    *name_field = if name_field.len() > 20 {
        format!("{}...", &name_field[..20])
    } else {
        name_field.to_string()
    };
}
fn timestamp_field_to_date ( timestamp_field: &mut String ) {
    let timestamp_i64 = timestamp_field.parse::<i64>().unwrap();
    *timestamp_field = if let Some(date_time) = chrono::DateTime::from_timestamp(timestamp_i64, 0) {
        date_time.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    } else {
        String::from("Invalid timestamp!")
    };
}

// Askama helper functions
fn to_i32 ( num: &&String ) -> Result<i32> {
    Ok(num.parse::<f64>()
        .context("Failed to parse number!")?
        as i32)
}
fn div_two_i32s_into_f32 ( num1: &&String, num2: &&String ) -> Result<f32> {
    let result = num1.parse::<f32>()
        .context("Failed to parse number 1!")?
        / num2.parse::<f32>()
            .context("Failed to parse number 2!")?;
    Ok(result)
}

#[tracing::instrument]
fn sort_jobs (
    jobs: &mut Vec<BTreeMap<String, String>>,
    sort_query: Option<&String>,
    reverse_query: Option<&String>,
    authenticated: bool
) {
    if sort_query.is_none() && reverse_query.is_none()  {
        return;
    }

    let sort_query = sort_query
        .and_then(|st| Some(st.as_str()))
        .and_then(|st| {
            if !authenticated {
                if st == "owner" {
                    return Some("pbs_id");
                }
            }

            Some(st)
        })
        .unwrap_or("pbs_id");
    let reverse_query = reverse_query
        .and_then(|st| Some(st.as_str()))
        .unwrap_or("false")
        .parse::<bool>()
        .unwrap_or(false);

    info!("Sorting jobs by {} in reverse: {}", sort_query, reverse_query);

    jobs.sort_by(|a, b| {
        let a = a.get(sort_query)
            .and_then(|st| Some(st.as_str()))
            .unwrap_or("0");
        let b = b.get(sort_query)
            .and_then(|st| Some(st.as_str()))
            .unwrap_or("0");

        // First, try to parse to a float
        if let (Ok(a), Ok(b)) = (a.parse::<f32>(), b.parse::<f32>()) {
            return a.partial_cmp(&b).unwrap();
        }

        // Second, if the sort query is 'req_walltime' or 'used_walltime', sort by HH:MM:SS
        if sort_query == "req_walltime" || sort_query == "used_walltime" {
            let a = a.split(':').collect::<Vec<&str>>();
            let b = b.split(':').collect::<Vec<&str>>();

            if a.len() != 3 || b.len() != 3 {
                return a.cmp(&b);
            }

            let a = a.iter()
                .map(|st| st.parse::<i32>().unwrap_or(0))
                .collect::<Vec<i32>>();
            let b = b.iter()
                .map(|st| st.parse::<i32>().unwrap_or(0))
                .collect::<Vec<i32>>();

            if a.len() != 3 || b.len() != 3 {
                return a.cmp(&b);
            }

            if a[0] != b[0] {
                return a[0].cmp(&b[0]);
            } else if a[1] != b[1] {
                return a[1].cmp(&b[1]);
            } else {
                return a[2].cmp(&b[2]);
            }
        }

        a.cmp(b)
    });

    if reverse_query {
        jobs.reverse();
    }
}
fn add_efficiency_tooltips ( job: &mut BTreeMap<String, String> ) {
    job.insert(
        String::from("cpu_efficiency_tooltip"),
        format!(
            "<b>CPU Efficiency: {:.2}%</b><br><br>", 
            job.get("cpu_efficiency")
                .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
                .unwrap_or(0f32)
        ) +
        match job.get("cpu_efficiency")
            .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
            .unwrap_or(0f32)
            .floor()
        {
            x if x < 50f32 => "Your job is not using the CPU efficiently! Consider using fewer CPUs.",
            x if x < 75f32 => "Your job is using the CPU somewhat efficiently.",
            x if x <= 100f32 => "Your job is using the CPU very efficiently!",
            _ => "Your job is using too much CPU! Consider allocating more CPUs."
        }
    );
    job.insert(
        String::from("mem_efficiency_tooltip"),
        format!(
            "<b>Memory Efficiency: {:.2}%</b><br><br>", 
            job.get("mem_efficiency")
                .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
                .unwrap_or(0f32)
        ) +
        match job.get("mem_efficiency")
            .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
            .unwrap_or(0f32)
            .floor()
        {
            x if x < 50f32 => "Your job is not using the memory efficiently! Consider using less memory. If you are using a GPU, this is okay.",
            x if x < 75f32 => "Your job is using the memory somewhat efficiently. If you are using a GPU, this is okay.",
            x if x <= 100f32 => "Your job is using the memory very efficiently!",
            _ => "Your job is using too much memory! Consider allocating more memory."
        }
    );
    job.insert(
        String::from("walltime_efficiency_tooltip"),
        format!(
            "<b>Walltime Efficiency: {:.2}%</b><br><br>",
            job.get("walltime_efficiency")
                .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
                .unwrap_or(0f32)
        ) +
        match job.get("walltime_efficiency")
            .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
            .unwrap_or(0f32)
            .floor()
        {
            x if x < 50f32 => "Your job is not using the walltime efficiently! Consider using less walltime.",
            x if x < 75f32 => "Your job is using the walltime somewhat efficiently.",
            x if x <= 100f32 => "Your job is using the walltime very efficiently!",
            _ => "Your job is using too much walltime! Consider allocating more walltime."
        }
    );
}
fn add_exit_status_tooltip ( job: &mut BTreeMap<String, String> ) {
    job.insert(
        String::from("exit_status_tooltip"),
        format!(
            "<b>Exit Status: {}</b><br><br>", 
            job.get("exit_status")
                .and_then(|st| Some(st.parse::<i32>().unwrap_or(0)) )
                .unwrap_or(0)
        )
    );
}