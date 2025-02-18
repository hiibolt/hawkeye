use std::collections::BTreeMap;
use tracing::info;
use anyhow::{Context, Result};

pub mod running;
pub mod login;
pub mod completed;
pub mod search;
pub mod stats;

#[derive(Clone)]
enum TableStat {
    Status,
    StartTime,
    EndTime,
    CpuTime,
    UsedMemPerCore,
    UsedMem,
    Queue,
    RsvdTime,
    RsvdCpus,
    RsvdGpus,
    RsvdMem,
    WalltimeEfficiency,
    CpuEfficiency,
    MemEfficiency,
    NodesChunks,
    #[allow(dead_code)]
    Custom {
        name: String,
        tooltip: String,
        sort_by: Option<String>,
        value: String,
        value_unit: Option<String>,
        colored: bool
    }
}
impl TableStat {
    fn adjust_job (
        &self,
        job: &mut BTreeMap<String, String>
    ) -> Result<()> {
        match self {
            TableStat::StartTime => {
                let start_time_str_ref = job.get_mut("start_time")
                    .context("Failed to get start time!")?;

                if start_time_str_ref == "2147483647" {
                    *start_time_str_ref = String::from("Not Started");
                } else {
                    timestamp_field_to_date(start_time_str_ref);
                }
            },
            TableStat::EndTime => {
                let end_time_str_ref = job.get_mut("end_time")
                    .context("Failed to get end time!")?;

                if end_time_str_ref == "2147483647" {
                    *end_time_str_ref = String::from("Not Ended");
                } else {
                    timestamp_field_to_date(end_time_str_ref);
                }
            },
            TableStat::UsedMemPerCore => {
                job.insert(
                    String::from("used_mem_per_cpu"),
                    ( job.get("used_mem")
                        .and_then(|st| st.parse::<f32>().ok())
                        .unwrap_or(0f32) /
                    job.get("req_cpus")
                        .and_then(|st| st.parse::<f32>().ok())
                        .unwrap_or(1f32) )
                        .to_string()
                );
            }
            TableStat::NodesChunks => {
                job.insert(
                    String::from("nodes/chunks"),
                    format!("{}/{}", 
                        job.get("nodes").unwrap_or(&"".to_string())
                            .split(',').collect::<Vec<&str>>().len(),
                        job.get("chunks").unwrap_or(&"0".to_string())
                    )
                );
            }
            TableStat::RsvdGpus => {
                if job.get("req_gpus").is_none() {
                    job.insert(String::from("req_gpus"), String::from("0"));
                }
            },
            TableStat::CpuEfficiency | TableStat::MemEfficiency => {
                add_efficiency_tooltips(job);
            },
            TableStat::WalltimeEfficiency => {
                add_efficiency_tooltips(job);

                let walltime_efficiency_ref = job.get_mut("walltime_efficiency")
                    .context("Failed to get walltime efficiency!")?;

                if let Ok(walltime_efficiency) = walltime_efficiency_ref.parse::<f32>() {
                    *walltime_efficiency_ref = format!("{}%", walltime_efficiency.ceil());
                }
            },
            TableStat::Custom { .. } => {
                // Do nothing
            },
            _ => {}
        };

        Ok(())
    }
    fn ensure_needed_field (
        &self,
        job: &mut BTreeMap<String, String>
    ) -> Result<()> {
        let value = Into::<TableEntry>::into(self.clone()).value;

        if job.get(&value).is_none() {
            return Err(anyhow::anyhow!("Field '{}' not found in job!", value));
        }

        Ok(())
    }
}
impl Into<TableEntry> for TableStat {
    fn into ( self ) -> TableEntry {
        match self {
            TableStat::Status => TableEntry {
                name: String::from("Status"),
                tooltip: String::from("<b>PBS Job State</b>"),
                sort_by: Some(String::from("state")),
                value: String::from("state"),
                value_unit: None,
                colored: false
            },
            TableStat::StartTime => TableEntry {
                name: String::from("Start Time"),
                tooltip: String::from("<b>Job Start Time</b><br><br>Not to be confused with submission time"),
                sort_by: Some(String::from("start_time")),
                value: String::from("start_time"),
                value_unit: None,
                colored: false
            },
            TableStat::EndTime => TableEntry {
                name: String::from("End Time"),
                tooltip: String::from("<b>Job End Time</b><br><br>Not to be confused with completion time"),
                sort_by: Some(String::from("end_time")),
                value: String::from("end_time"),
                value_unit: None,
                colored: false
            },
            TableStat::CpuTime => TableEntry {
                name: String::from("CPU Time"),
                tooltip: String::from("<b>Total CPU Time</b><br><br>The total amount of CPU time used by the job"),
                sort_by: Some(String::from("used_cpu_time")),
                value: String::from("used_cpu_time"),
                value_unit: None,
                colored: false
            },
            TableStat::UsedMemPerCore => TableEntry {
                name: String::from("Mem/Core"),
                tooltip: String::from("<b>Memory per Core</b><br><br>The amount of memory used per CPU core, in GB"),
                sort_by: Some(String::from("used_mem_per_cpu")),
                value: String::from("used_mem_per_cpu"),
                value_unit: Some(String::from("GB")),
                colored: false
            },
            TableStat::UsedMem => TableEntry {
                name: String::from("Used Mem"),
                tooltip: String::from("<b>Used Memory</b><br><br>The total amount of memory used by the job, in GB"),
                sort_by: Some(String::from("used_mem")),
                value: String::from("used_mem"),
                value_unit: Some(String::from("GB")),
                colored: false
            },
            TableStat::Queue => TableEntry {
                name: String::from("Queue"),
                tooltip: String::from("<b>Job Queue</b><br><br>The queue in which the job was designated"),
                sort_by: Some(String::from("queue")),
                value: String::from("queue"),
                value_unit: None,
                colored: false
            },
            TableStat::RsvdTime => TableEntry {
                name: String::from("Rsvd Time"),
                tooltip: String::from("<b>The amount of reserved walltime</b>"),
                sort_by: Some(String::from("req_walltime")),
                value: String::from("req_walltime"),
                value_unit: None,
                colored: false
            },
            TableStat::RsvdCpus => TableEntry {
                name: String::from("Rsvd CPUs"),
                tooltip: String::from("<b>The number of reserved CPU cores</b>"),
                sort_by: Some(String::from("req_cpus")),
                value: String::from("req_cpus"),
                value_unit: None,
                colored: false
            },
            TableStat::RsvdGpus => TableEntry {
                name: String::from("Rsvd GPUs"),
                tooltip: String::from("<b>The number of reserved GPU cards</b>"),
                sort_by: Some(String::from("req_gpus")),
                value: String::from("req_gpus"),
                value_unit: None,
                colored: false
            },
            TableStat::RsvdMem => TableEntry {
                name: String::from("Rsvd Mem"),
                tooltip: String::from("<b>The amount of reserved RAM, in GB</b>"),
                sort_by: Some(String::from("req_mem")),
                value: String::from("req_mem"),
                value_unit: Some(String::from("GB")),
                colored: false
            },
            TableStat::WalltimeEfficiency => TableEntry {
                name: String::from("Elapsed Walltime"),
                tooltip: String::from("<b>Total elapsed walltime/Reserved walltime, in %"),
                sort_by: Some(String::from("walltime_efficiency")),
                value: String::from("walltime_efficiency"),
                value_unit: None,
                colored: false
            },
            TableStat::CpuEfficiency => TableEntry {
                name: String::from("CPU Usage"),
                tooltip: String::from("<b>CPU Usage Efficiency</b><br><br>The integral load of all CPUs in use divided by the number of reserved CPUs, in %.<br><br>If low (indicated by the reddish background), consider a <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#jobcontrol\">workflow optimization</a>."),
                sort_by: Some(String::from("cpu_efficiency")),
                value: String::from("cpu_efficiency"),
                value_unit: None,
                colored: true
            },
            TableStat::MemEfficiency => TableEntry {
                name: String::from("Memory Usage"),
                tooltip: String::from("<b>Memory Usage Efficiency</b><br><br>The total amount of memory in use divided by the amount of reserved memory, in %.<br><br>If low (indicated by the reddish background), consider a <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#jobcontrol\">workflow optimization</a>."),
                sort_by: Some(String::from("mem_efficiency")),
                value: String::from("mem_efficiency"),
                value_unit: None,
                colored: true
            },
            TableStat::NodesChunks => TableEntry {
                name: String::from("Nodes/Chunks"),
                tooltip: String::from("<b>Number of Nodes/Chunks</b><br><br>The number of nodes and chunks used by the job"),
                sort_by: None,
                value: String::from("nodes/chunks"),
                value_unit: None,
                colored: false
            },
            TableStat::Custom { name, tooltip, sort_by, value, value_unit, colored } => TableEntry {
                name,
                tooltip,
                sort_by,
                value,
                value_unit,
                colored
            }
        }
    }
}

#[derive(Debug)]
struct TableEntry {
    name: String,
    tooltip: String,
    sort_by: Option<String>,
    value: String,
    value_unit: Option<String>,
    colored: bool,
}

// Field helper functions
fn timestamp_field_to_date ( timestamp_field: &mut String ) {
    let timestamp_i64 = timestamp_field.parse::<i64>().unwrap();
    *timestamp_field = if let Some(date_time) = chrono::DateTime::from_timestamp(timestamp_i64, 0) {
        date_time.with_timezone(&chrono::Local)
            .format("%b %e, %Y at %l:%M%p")
            .to_string()
    } else {
        String::from("Invalid timestamp!")
    };
}

// Askama helper functions
fn to_i32 ( num: &&String ) -> Result<i32> {
    Ok(num.parse::<f64>()
        .context("Failed to parse number!")?
        .ceil()
        as i32)
}
fn div_two_i32s_into_f32 ( num1: &&String, num2: &&String ) -> Result<f32> {
    let result = num1.parse::<f32>()
        .context("Failed to parse number 1!")?
        / num2.parse::<f32>()
            .context("Failed to parse number 2!")?;
    Ok(result)
}
fn shorten ( name_field: &&String ) -> String {
    if name_field.len() > 18 {
        format!("{}...", &name_field[..18])
    } else {
        (*name_field).clone()
    }
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
        if sort_query == "req_walltime" || sort_query == "used_walltime" || sort_query == "used_cpu_time" {
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
    let cpu_efficiency = job.get("cpu_efficiency")
        .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
        .unwrap_or(0f32);
    let mem_efficiency = job.get("mem_efficiency")
        .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
        .unwrap_or(0f32);
    let walltime_efficiency = job.get("walltime_efficiency")
        .and_then(|st| Some(st.parse::<f32>().unwrap_or(0f32)) )
        .unwrap_or(0f32);

    job.insert(
        String::from("cpu_efficiency_tooltip"),
        format!("<b>CPU Efficiency: {cpu_efficiency:.2}%</b>")
        + "<br><br>"
        + match cpu_efficiency {
            x if x < 50f32 => "Your job has a low CPU load, consider reserving fewer CPUs.",
            x if x < 75f32 => "Your job is using the CPU somewhat efficiently.",
            x if x >= 75f32 => "Your job is using the CPU very efficiently!",
            _ => "Abnormal CPU usage!"
        } 
        + "<br><br>"
        + "See the bottom of the <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#Jobs%20optimization%20and%20control\">CRCD docs</a> for more."
    );
    job.insert(
        String::from("mem_efficiency_tooltip"),
        format!("<b>Memory Efficiency: {mem_efficiency:.2}%</b>")
        + "<br><br>"
        + match mem_efficiency {
            x if x < 50f32 => "Your job has low memory utilization, consider reserving less memory. If you are using a GPU, this is okay.",
            x if x < 75f32 => "Your job is using the memory somewhat efficiently. If you are using a GPU, this is okay.",
            x if x >= 75f32 => "Your job is using the memory very efficiently!",
            _ => "Abnormal memory usage!"
        }
        + "<br><br>"
        + "See the bottom of the <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#Jobs%20optimization%20and%20control\">CRCD docs</a> for more."
    );
    job.insert(
        String::from("walltime_efficiency_tooltip"),
        format!("<b>Walltime Efficiency: {walltime_efficiency:.2}%</b>") 
        + "<br><br>"
        + match walltime_efficiency {
            x if x < 50f32 => "Your job didn't use most of its wallitme, consider using less walltime to help queue times.",
            x if x < 75f32 => "Your job is using the walltime somewhat efficiently.",
            x if x >= 75f32 => "Your job is using the walltime very efficiently!",
            _ => "Abnormal walltime usage!"
        }
        + "<br><br>"
        + "See the bottom of the <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#Jobs%20optimization%20and%20control\">CRCD docs</a> for more."
    );
}
fn signal_to_str_suffix ( 
    signal: i32
) -> &'static str {
    match signal {
        1 => " (SIGHUP)",
        2 => " (SIGINT)",
        3 => " (SIGQUIT)",
        4 => " (SIGILL)",
        5 => " (SIGTRAP)",
        6 => " (SIGABRT)",
        7 => " (SIGBUS)",
        _ => ""
    }
}
fn add_exit_status_tooltip ( job: &mut BTreeMap<String, String> ) {
    let exit_status = job.get("exit_status")
        .and_then(|st| Some(st.parse::<i32>().unwrap_or(0)) )
        .unwrap_or(0);

    job.insert(
        String::from("exit_status_tooltip"),
        format!("<b>Exit Status: {exit_status}</b><br><br>") + 
        match exit_status {
            0 => "The job executed successfully.",
            -1 => "Job exec failed, before files, no retry",
            -2 => "Job exec failed, after files, no retry",
            -3 => "Job exec failed, do retry",
            -4 => "Job aborted on MOM initialization",
            -5 => "Job aborted on MOM initialization, checkpoint, no migrate",
            -6 => "Job aborted on MOM initialization, checkpoint, ok migrate",
            -7 => "Job restart failed",
            -8 => "Initialization of Globus job failed. Do retry.",
            -9 => "Initialization of Globus job failed. Do not retry.",
            -10 => "Invalid UID/GID for job",
            -11 => "Job was rerun",
            -12 => "Job was checkpointed and killed",
            -13 => "Job failed due to a bad password",
            -14 => "Job was requeued (if rerunnable) or deleted (if not) due to a communication failure between Mother Superior and a Sister",
            x if x < 128 && x > 0 => "The exit value of the top process in the job, typically the shell.",
            x if x >= 128 => { 
                "" // Computed in next if statement (to
                   //  avoid referencing a dropped value)
            },
            _ => "Unknown exit status."
        } + 
        &if exit_status >= 128 { // Borrow here to treat as `&str`
            let signal = exit_status % 128;
            format!(
                "The job's top process was killed with signal {}{}.",
                signal,
                signal_to_str_suffix(signal)
            )
        } else {
            String::new()
        } + 
        "<br><br>" +
        "<a href=\"https://www.nas.nasa.gov/hecc/support/kb/pbs-exit-codes_185.html\">More information on PBS exit codes</a>"
    );
}