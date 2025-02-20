use std::collections::{BTreeMap, HashMap, HashSet};
use axum::{http::{self, StatusCode}, response::Response};
use tracing::{error, info};
use anyhow::{Context, Result};

pub mod running;
pub mod login;
pub mod completed;
pub mod search;
pub mod stats;

#[derive(Clone, Debug)]
enum TableStatType {
    Default,

    Colored,

    JobID,
    JobName(usize),
    JobGroups(usize),
    JobOwner,
    ExitStatus,
    More
}
#[derive(Clone, Debug)]
enum TableStat {
    JobID,
    JobOwner,
    JobName(usize),
    JobGroups(usize),
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
    ElapsedWalltime,
    ElapsedWalltimeColored,
    CpuEfficiency,
    MemEfficiency,
    NodesChunks,
    ExitStatus,
    More,
    #[allow(dead_code)]
    Custom {
        name: String,
        tooltip: String,
        sort_by: Option<String>,
        value: String,
        value_unit: Option<String>,
        stat_type: TableStatType
    }
}
impl TableStat {
    fn adjust_job (
        &self,
        group_cache: &HashMap<String, HashSet<String>>,
        job: &mut BTreeMap<String, String>
    ) -> Result<()> {
        match self {
            TableStat::JobGroups(_) => {
                let owner = job.get("owner")
                    .context("Missing `owner` field!")?;

                if owner == "REDACTED" {
                    job.insert(
                        String::from("groups"),
                        String::from("REDACTED")
                    );
                    
                    return Ok(());
                }

                job.insert(
                    String::from("groups"),
                    group_cache.get(owner)
                        .unwrap_or(&HashSet::from([String::from("None")]))
                        .into_iter()
                        .map(|st| st.to_owned())
                        .collect::<Vec<String>>()
                        .join(",")
                );
            }
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
                    format!("{:.2}",
                        ( job.get("used_mem")
                            .and_then(|st| st.parse::<f32>().ok())
                            .unwrap_or(0f32) /
                        job.get("req_cpus")
                            .and_then(|st| st.parse::<f32>().ok())
                            .unwrap_or(1f32) )
                    )
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
            TableStat::ElapsedWalltime | TableStat::ElapsedWalltimeColored => {
                add_efficiency_tooltips(job);

                let walltime_efficiency_ref = job.get_mut("walltime_efficiency")
                    .context("Failed to get walltime efficiency!")?;

                if let Ok(walltime_efficiency) = walltime_efficiency_ref.parse::<f32>() {
                    *walltime_efficiency_ref = format!("{}", walltime_efficiency.ceil());
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
            TableStat::JobID => TableEntry {
                name: String::from("Job ID"),
                tooltip: String::from("<b>PBS Job ID</b>"),
                sort_by: Some(String::from("pbs_id")),
                value: String::from("pbs_id"),
                value_unit: None,
                stat_type: TableStatType::JobID
            },
            TableStat::JobOwner => TableEntry {
                name: String::from("Job Owner"),
                tooltip: String::from("<b>The UNIX Username of the Job Owner</b>"),
                sort_by: Some(String::from("owner")),
                value: String::from("owner"),
                value_unit: None,
                stat_type: TableStatType::JobOwner
            },
            TableStat::JobName(len) => TableEntry {
                name: String::from("Job Name"),
                tooltip: String::from("<b>Job Name</b>"),
                sort_by: Some(String::from("name")),
                value: String::from("name"),
                value_unit: None,
                stat_type: TableStatType::JobName(len)
            },
            TableStat::JobGroups(len) => TableEntry {
                name: String::from("Groups"),
                tooltip: String::from("<b>Job Groups</b>"),
                sort_by: None,
                value: String::from("groups"),
                value_unit: None,
                stat_type: TableStatType::JobGroups(len)
            },
            TableStat::Status => TableEntry {
                name: String::from("Status"),
                tooltip: String::from("<b>PBS Job State</b>"),
                sort_by: Some(String::from("state")),
                value: String::from("state"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::StartTime => TableEntry {
                name: String::from("Start Time"),
                tooltip: String::from("<b>Job Start Time</b><br><br>Not to be confused with submission time"),
                sort_by: Some(String::from("start_time")),
                value: String::from("start_time"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::EndTime => TableEntry {
                name: String::from("End Time"),
                tooltip: String::from("<b>Job End Time</b><br><br>Not to be confused with completion time"),
                sort_by: Some(String::from("end_time")),
                value: String::from("end_time"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::CpuTime => TableEntry {
                name: String::from("CPU Time"),
                tooltip: String::from("<b>Total CPU Time</b><br><br>The total amount of CPU time used by the job"),
                sort_by: Some(String::from("used_cpu_time")),
                value: String::from("used_cpu_time"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::UsedMemPerCore => TableEntry {
                name: String::from("Mem/Core"),
                tooltip: String::from("<b>Memory per Core</b><br><br>The amount of memory used per CPU core, in GB"),
                sort_by: None,
                value: String::from("used_mem_per_cpu"),
                value_unit: Some(String::from("GB")),
                stat_type: TableStatType::Default
            },
            TableStat::UsedMem => TableEntry {
                name: String::from("Used Mem"),
                tooltip: String::from("<b>Used Memory</b><br><br>The total amount of memory used by the job, in GB"),
                sort_by: Some(String::from("used_mem")),
                value: String::from("used_mem"),
                value_unit: Some(String::from("GB")),
                stat_type: TableStatType::Default
            },
            TableStat::Queue => TableEntry {
                name: String::from("Queue"),
                tooltip: String::from("<b>Job Queue</b><br><br>The queue in which the job was designated"),
                sort_by: Some(String::from("queue")),
                value: String::from("queue"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::RsvdTime => TableEntry {
                name: String::from("Rsvd Time"),
                tooltip: String::from("<b>The amount of reserved walltime</b>"),
                sort_by: Some(String::from("req_walltime")),
                value: String::from("req_walltime"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::RsvdCpus => TableEntry {
                name: String::from("Rsvd CPUs"),
                tooltip: String::from("<b>The number of reserved CPU cores</b>"),
                sort_by: Some(String::from("req_cpus")),
                value: String::from("req_cpus"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::RsvdGpus => TableEntry {
                name: String::from("Rsvd GPUs"),
                tooltip: String::from("<b>The number of reserved GPU cards</b>"),
                sort_by: Some(String::from("req_gpus")),
                value: String::from("req_gpus"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::RsvdMem => TableEntry {
                name: String::from("Rsvd Mem"),
                tooltip: String::from("<b>The amount of reserved RAM, in GB</b>"),
                sort_by: Some(String::from("req_mem")),
                value: String::from("req_mem"),
                value_unit: Some(String::from("GB")),
                stat_type: TableStatType::Default
            },
            TableStat::ElapsedWalltime => TableEntry {
                name: String::from("Elapsed Walltime"),
                tooltip: String::from("<b>Total elapsed walltime/Reserved walltime, in %"),
                sort_by: Some(String::from("walltime_efficiency")),
                value: String::from("walltime_efficiency"),
                value_unit: Some(String::from("%")),
                stat_type: TableStatType::Default
            },
            TableStat::ElapsedWalltimeColored => TableEntry {
                name: String::from("Elapsed Walltime"),
                tooltip: String::from("<b>Total elapsed walltime/Reserved walltime, in %"),
                sort_by: Some(String::from("walltime_efficiency")),
                value: String::from("walltime_efficiency"),
                value_unit: None,
                stat_type: TableStatType::Colored
            },
            TableStat::CpuEfficiency => TableEntry {
                name: String::from("CPU Usage"),
                tooltip: String::from("<b>CPU Usage Efficiency</b><br><br>The integral load of all CPUs in use divided by the number of reserved CPUs, in %.<br><br>If low (indicated by the reddish background), consider a <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#jobcontrol\">workflow optimization</a>."),
                sort_by: Some(String::from("cpu_efficiency")),
                value: String::from("cpu_efficiency"),
                value_unit: None,
                stat_type: TableStatType::Colored
            },
            TableStat::MemEfficiency => TableEntry {
                name: String::from("Memory Usage"),
                tooltip: String::from("<b>Memory Usage Efficiency</b><br><br>The total amount of memory in use divided by the amount of reserved memory, in %.<br><br>If low (indicated by the reddish background), consider a <a href=\"https://www.niu.edu/crcd/current-users/getting-started/queue-commands-job-management.shtml#jobcontrol\">workflow optimization</a>."),
                sort_by: Some(String::from("mem_efficiency")),
                value: String::from("mem_efficiency"),
                value_unit: None,
                stat_type: TableStatType::Colored
            },
            TableStat::NodesChunks => TableEntry {
                name: String::from("Nodes/Chunks"),
                tooltip: String::from("<b>Number of Nodes/Chunks</b><br><br>The number of nodes and chunks used by the job"),
                sort_by: None,
                value: String::from("nodes/chunks"),
                value_unit: None,
                stat_type: TableStatType::Default
            },
            TableStat::ExitStatus => TableEntry {
                name: String::from("Exit Status"),
                tooltip: String::from("<b>Exit Status</b><br><br>The exit status of the job"),
                sort_by: Some(String::from("exit_status")),
                value: String::from("exit_status"),
                value_unit: None,
                stat_type: TableStatType::ExitStatus
            },
            TableStat::More => TableEntry {
                name: String::from("More"),
                tooltip: String::from("<b>More Information</b>"),
                sort_by: None,
                value: String::from("pbs_id"),
                value_unit: None,
                stat_type: TableStatType::More
            },
            TableStat::Custom { name, tooltip, sort_by, value, value_unit, stat_type } => TableEntry {
                name,
                tooltip,
                sort_by,
                value,
                value_unit,
                stat_type
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
    stat_type: TableStatType,
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
#[derive(Debug)]
struct Toolkit;
impl Toolkit {
    pub fn total_successful_jobs (
        &self,
        jobs: &Vec<BTreeMap<String, String>>
    ) -> usize {
        jobs.iter()
            .filter(|job| job.get("exit_status").unwrap_or(&"1".to_string()) == "0")
            .count()
    }
    pub fn total_cpu_time (
        &self,
        jobs: &Vec<BTreeMap<String, String>>
    ) -> String {
        let mut total_hours = 0;
        let mut total_minutes: i32 = 0;
        let mut total_seconds: i32 = 0;

        for job in jobs.iter() {
            let time = job.get("used_cpu_time")
                .and_then(|st| Some(st.to_owned()))
                .unwrap_or(String::from("0"));
            
            if let Some(hours) = time.split(':').nth(0) {
                total_hours += hours.parse::<i32>().unwrap_or(0)
            } else {
                continue;
            }
            if let Some(minutes) = time.split(':').nth(1) {
                total_minutes += minutes.parse::<i32>().unwrap_or(0)
            } else {
                continue;
            }
            if let Some(seconds) = time.split(':').nth(2) {
                total_seconds += seconds.parse::<i32>().unwrap_or(0)
            } else {
                continue;
            }
        }

        total_minutes += total_seconds / 60;
        total_seconds %= 60;
        total_hours += total_minutes / 60;
        total_minutes %= 60;

        let total_days = total_hours / 24;
        total_hours %= 24;

        format!("{:02}:{:02}:{:02}:{:02}", total_days, total_hours, total_minutes, total_seconds)
    }
    pub fn to_i32 ( &self, num: &&String ) -> Result<i32> {
        Ok(num.parse::<f64>()
            .context("Failed to parse number!")?
            .ceil()
            as i32)
    }
    pub fn div_two_i32s_into_f32 ( &self, num1: &&String, num2: &&String ) -> Result<f32> {
        let result = num1.parse::<f32>()
            .context("Failed to parse number 1!")?
            / num2.parse::<f32>()
                .context("Failed to parse number 2!")?;
        Ok(result)
    }
    pub fn shorten ( &self, name_field: &&String, len: &usize ) -> String {
        if name_field.len() > *len {
            format!("{}...", &name_field[..(*len-4)])
        } else {
            (*name_field).clone()
        }
    }
    pub fn get_field ( &self, job: &BTreeMap<String, String>, field: &str ) -> Result<String> {
        job.get(field)
            .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in job!", field))
            .map(|st| st.to_string())
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
        1  => " (SIGHUP)",
        2  => " (SIGINT)",
        3  => " (SIGQUIT)",
        4  => " (SIGILL)",
        5  => " (SIGTRAP)",
        6  => " (SIGABRT)",
        7  => " (SIGBUS)",
        8  => " (SIGFPE)",
        9  => " (SIGKILL)",
        15 => " (SIGTERM)",
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
fn try_render_template <T: ?Sized + askama::Template> (
    template: &T
) -> Result<Response, (StatusCode, String)> {
    let value = template.render()
        .map_err(|err| {
            error!(%err, "Failed to render template!");
            (
                StatusCode::INTERNAL_SERVER_ERROR, 
                format!("Failed to render template!\n\nError: {err}")
            )
        })?
        .into();
    Response::builder()
        .header(
            http::header::CONTENT_TYPE,
            http::header::HeaderValue::from_static(T::MIME_TYPE),
        )
        .body(value)
        .map_err(|err| {
            error!(%err, "Failed to build response!");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build response!".to_string())
        })
}
#[tracing::instrument]
    fn sort_build_parse (
        groups_cache: HashMap<String, HashSet<String>>,
        table_stats: Vec<TableStat>,

        jobs: &mut Vec<BTreeMap<String, String>>,
        params: &HashMap<String, String>,
        username: Option<String>
    ) -> (
        Vec<TableEntry>, // Table entries
        Option<String>,  // Error string
    ) {
        // Sort the jobs by any sort and reverse queries
        sort_jobs(
            jobs,
            params.get("sort"),
            params.get("reverse"),
            username.is_some()
        );

        // Tweak data to be presentable and add tooltips for efficiencies
        let mut errors = Vec::new();
        for job_ref in jobs.iter_mut() {
            // Add tooltip for exit status
            add_exit_status_tooltip(job_ref);

            for table_stat in table_stats.iter() {
                if let Err(e) = table_stat.adjust_job(&groups_cache, job_ref) {
                    errors.push(e);
                }
                if let Err(e) = table_stat.ensure_needed_field(job_ref) {
                    errors.push(e);
                }
            }
        }
        let errors = errors
            .iter()
            .map(|e| e.to_string())
            .enumerate()
            .map(|(i, e)| format!("{}. {}", i + 1, e))
            .collect::<Vec<String>>()
            .join("\n");

        // If there are errors, wipe the jobs
        if !errors.is_empty() {
            jobs.clear();

            // Print the errors if there are any
            error!(%errors, "Errors while parsing jobs!");
        }

        // Reverse the results
        jobs.reverse();

        (
            table_stats.into_iter()
                .map(|table_stat| table_stat.into() )
                .collect(),
            (!errors.trim().is_empty()).then_some(errors)
        )
    }