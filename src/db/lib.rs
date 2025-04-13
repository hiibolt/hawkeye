use std::{collections::{BTreeMap, HashMap, HashSet}, time::{SystemTime, UNIX_EPOCH}};

use chrono::{DateTime, Utc};
use anyhow::{Context, Result, anyhow};
use rusqlite::{params, params_from_iter, Connection};
use tracing::{info, error};
use tokio::sync::Mutex;

use super::super::remote::auth::verify_login;

#[derive(Debug)]
pub struct DB {
    conn: Mutex<Connection>,
    groups_cache: Mutex<HashMap<String, HashSet<String>>>
}
pub struct LoginResult {
    pub success: bool,
    pub created_new: bool
}

impl DB {
    pub fn new (
        path: &str
    ) -> Result<Self> {
        let conn = Connection::open(path)
            .context("Failed to establish connection to DB!")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS Groups (
                name TEXT PRIMARY KEY
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS Users (
                name TEXT PRIMARY KEY
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS UserGroups (
                user_name TEXT NOT NULL,
                group_name TEXT NOT NULL,
                PRIMARY KEY (user_name, group_name),
                FOREIGN KEY (user_name) REFERENCES Users(name) ON DELETE CASCADE,
                FOREIGN KEY (group_name) REFERENCES Groups(name) ON DELETE CASCADE
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS Jobs (
                pbs_id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                owner TEXT NOT NULL,
                state TEXT NOT NULL,
                start_time INTEGER NOT NULL,
                queue TEXT NOT NULL,
                nodes TEXT NOT NULL,
                req_mem REAL NOT NULL,
                req_cpus INTEGER NOT NULL,
                req_gpus INTEGER NOT NULL,
                req_walltime TEXT NOT NULL,
                req_select TEXT NOT NULL,
                mem_efficiency REAL NOT NULL,
                walltime_efficiency REAL NOT NULL,
                cpu_efficiency REAL NOT NULL,
                used_cpu_percent REAL NOT NULL,
                used_mem REAL NOT NULL,
                used_walltime TEXT NOT NULL,
                end_time INTEGER NOT NULL,
                chunks TEXT NOT NULL,
                exit_status TEXT NOT NULL,
                est_start_time TEXT NOT NULL,
                used_cpu_time TEXT NOT NULL,
                FOREIGN KEY (owner) REFERENCES Users(owner)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS PastStats (
                stat_id INTEGER PRIMARY KEY AUTOINCREMENT,
                pbs_id INTEGER NOT NULL,
                cpu_percent REAL NOT NULL,
                mem REAL NOT NULL,
                datetime STRING NOT NULL,
                FOREIGN KEY (pbs_id) REFERENCES Jobs(pbs_id)
            )",
            [],
        )?;
        
        Ok(Self {
            conn: Mutex::new(conn),
            groups_cache: Mutex::new(HashMap::new())
        })
    }
    pub async fn insert_job (
        &self,
        job: &BTreeMap<&str, String>
    ) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR IGNORE INTO Users (name) VALUES (?1)",
            [&job.get("Job_Owner").context("Missing job owner")?],
        )?;

        // Fix the `Resource_List.select` field (add `nchunks=` to the beginning)
        //  so that `1:ncpus=32:mpiprocs=32:ngpus=1:mem=50gb` becomes 
        //  `nchunks=1:ncpus=32:mpiprocs=32:ngpus=1:mem=50gb`
        let select = job.get("Resource_List.select").context("Missing job select")?;
        let select = if select.starts_with("nchunks=") {
            select.to_string()
        } else {
            format!("nchunks={select}")
        };

        // If there are no chunks, get it from the `Resource_List.select` field
        let chunks = match job.get("chunks") {
            Some(chunks) => chunks.to_string(),
            None => {
                select.split('=')
                    .nth(1).context("Missing chunks value in select statement (1)")?
                    .split(':')
                    .next().context("Missing chunks value in select statement (2)")?
                    .to_string()
            }
        };
        
        // Add the job
        conn.execute(
            "INSERT OR REPLACE INTO Jobs (pbs_id, name, owner, state, start_time, queue, nodes, req_mem, req_cpus, req_gpus, req_walltime, req_select, mem_efficiency, walltime_efficiency, cpu_efficiency, used_cpu_percent, used_mem, used_walltime, end_time, chunks, exit_status, est_start_time, used_cpu_time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                job.get("job_id").context("Missing job ID")?,
                job.get("Job_Name").context("Missing job name")?,
                job.get("Job_Owner").context("Missing job owner")?,
                job.get("job_state").context("Missing job state")?,
                job.get("start_time").context("Missing job start time")?,
                job.get("queue").context("Missing job queue")?,
                job.get("Nodes").context("Missing job nodes")?,
                job.get("Resource_List.mem").context("Missing job memory")?,
                job.get("Resource_List.ncpus").unwrap_or(&String::from("0")),
                job.get("Resource_List.ngpus").unwrap_or(&String::from("0")),
                job.get("Resource_List.walltime").context("Missing job walltime")?,
                select,
                job.get("mem_efficiency").context("Missing job memory efficiency")?,
                job.get("walltime_efficiency").context("Missing job walltime efficiency")?,
                job.get("cpu_efficiency").context("Missing job CPU efficiency")?,
                job.get("resources_used.cpupercent").context("Missing job CPU percent")?,
                job.get("resources_used.mem").context("Missing job used memory")?,
                job.get("resources_used.walltime").context("Missing job used walltime")?,
                job.get("end_time").unwrap_or(&i32::MAX.to_string()),
                chunks,
                job.get("Exit_status").unwrap_or(&String::from("Not Yet Completed")),
                job.get("estimated.start_time").unwrap_or(&String::from("Already Started/Unknown")),
                job.get("resources_used.cput").unwrap_or(&String::from("00:00:00")),
            ],
        )?;
        
        // Add the latest stats
        // Get the current system time
        let now = SystemTime::now();
        let duration_since_epoch = now.duration_since(UNIX_EPOCH)
            .context("Time went backwards")?;
        let datetime = DateTime::<Utc>::from(UNIX_EPOCH + duration_since_epoch);
        let formatted_datetime = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO PastStats (pbs_id, cpu_percent, mem, datetime) VALUES (?1, ?2, ?3, ?4)",
            params![
                job.get("job_id").context("Missing job ID")?,
                job.get("cpu_efficiency").context("Missing job CPU efficiency")?,
                job.get("resources_used.mem").context("Missing job used memory")?,
                formatted_datetime
            ],
        )?;

        Ok(())
    }

    pub async fn get_groups_cache (
        &self
    ) -> HashMap<String, HashSet<String>> {
        return self.groups_cache.lock().await.clone();
    }
    pub async fn insert_user_groups (
        &self,
        user: &str,
        groups: Vec<&str>
    ) -> Result<()> {
        self.groups_cache.lock().await.insert(
            user.to_string(),
            groups.iter()
                .map(|&st| st.to_string())
                .collect()
        );

        for group in groups {
            self.insert_user_group(&user, group)
                .await
                .with_context(|| format!("Couldn't insert user {user} into group {group}!"))?;
        }

        Ok(())
    }
    pub async fn insert_user_group (
        &self,
        user: &str,
        group: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().await;

        conn.execute(
            "INSERT OR IGNORE INTO Groups (name) VALUES (?1)",
            [group],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO UserGroups (user_name, group_name) VALUES (?1, ?2)",
            [user, group],
        )?;

        Ok(())
    }
    
    /// Update all jobs in state 'R' that are *not* in the current list of active jobs.
    /// We mark them as 'S' in the database (i.e., 'stopped' or 'completed').
    #[tracing::instrument]
    pub async fn mark_completed_jobs(
        &self,
        active_jobs: &[BTreeMap<&str, String>],
    ) -> Result<()> {
        let conn = self.conn.lock().await;

        // Build a set of IDs for *currently active* jobs
        let active_ids: HashSet<_> = active_jobs
            .iter()
            .flat_map(|job| Some(job.get("job_id")?.parse::<i32>().ok()?))
            .collect();
        
        // Find all jobs that are in state 'R' in our local DB
        let mut stmt = conn.prepare("SELECT pbs_id FROM Jobs WHERE state = 'R'")?;
        let rows = stmt.query_map([], |row| row.get::<_, i32>(0))?;
    
        // For each job in state 'R', check if it's still active
        for row_result in rows {
            let pbs_id = row_result?;
            // If a job's ID is *not* in the active set, we assume it completed
            if !active_ids.contains(&pbs_id) {
                info!("[ Marking job {} as completed... ]", pbs_id);

                let now = SystemTime::now();
                let secs_since_epoch = now.duration_since(UNIX_EPOCH)
                    .context("Time went backwards")?
                    .as_secs();

                conn.execute(
                    "UPDATE Jobs SET state = 'E' WHERE pbs_id = ?1",
                    [pbs_id],
                )?;

                conn.execute(
                    "UPDATE Jobs SET end_time = ?1 WHERE pbs_id = ?2",
                    [secs_since_epoch.to_string(), pbs_id.to_string()],
                )?;
            }
        }
    
        Ok(())
    }

    #[tracing::instrument]
    pub async fn get_user_jobs(
        &self,
        username: &str,
        filter_state: Option<&String>,
        filter_queue: Option<&String>,
        filter_owner: Option<&String>,
        filter_name: Option<&String>,
        filter_date: Option<&String>
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let conn = self.conn.lock().await;

        let mut additional_filters= String::new();
        let mut params = vec![username.to_string()];
        if let Some(filter_state) = filter_state {
            additional_filters.push_str(" AND state = ?2");
            params.push(filter_state.to_owned());
        }
        if let Some(filter_queue) = filter_queue {
            additional_filters.push_str(&format!(" AND queue = ?{}", params.len() + 1));
            params.push(filter_queue.to_owned());
        }
        if let Some(filter_owner) = filter_owner {
            additional_filters.push_str(&format!(" AND owner = ?{}", params.len() + 1));
            params.push(filter_owner.to_owned());
        }
        if let Some(filter_name) = filter_name {
            additional_filters.push_str(&format!(" AND name = ?{}", params.len() + 1));
            params.push(filter_name.to_owned());
        }
        // Make sure that the job is before or on the specified date,
        //  OR has not completed (state = R).
        if let Some(filter_date) = filter_date {
            info!("Filtering by date: {filter_date}");
            additional_filters.push_str(&format!(" AND start_time >= ?{}", params.len() + 1));
            params.push(filter_date.to_owned());
        }

        let mut stmt = conn.prepare(&format!("SELECT * FROM Jobs WHERE owner = ?1{}", additional_filters))?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), row.get::<_, String>(2)?),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("start_time".to_string(), row.get::<_, i32>(4)?.to_string()),
                ("queue".to_string(), row.get::<_, String>(5)?),
                ("nodes".to_string(), row.get::<_, String>(6)?),
                ("req_mem".to_string(), row.get::<_, f64>(7)?.to_string()),
                ("req_cpus".to_string(), row.get::<_, i32>(8)?.to_string()),
                ("req_gpus".to_string(), row.get::<_, i32>(9)?.to_string()),
                ("req_walltime".to_string(), row.get::<_, String>(10)?),
                ("req_select".to_string(), row.get::<_, String>(11)?),
                ("mem_efficiency".to_string(), row.get::<_, f64>(12)?.to_string()),
                ("walltime_efficiency".to_string(), row.get::<_, f64>(13)?.to_string()),
                ("cpu_efficiency".to_string(), row.get::<_, f64>(14)?.to_string()),
                ("used_cpu_percent".to_string(), row.get::<_, f64>(15)?.to_string()),
                ("used_mem".to_string(), row.get::<_, f64>(16)?.to_string()),
                ("used_walltime".to_string(), row.get::<_, String>(17)?),
                ("end_time".to_string(), row.get::<_, i32>(18)?.to_string()),
                ("chunks".to_string(), row.get::<_, String>(19)?),
                ("exit_status".to_string(), row.get::<_, String>(20)?),
                ("est_start_time".to_string(), row.get::<_, String>(21)?),
                ("used_cpu_time".to_string(), row.get::<_, String>(22)?),
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }

    #[tracing::instrument]
    pub async fn get_all_jobs (
        &self,
        filter_states: Option<Vec<&str>>,
        filter_queue: Option<&String>,
        filter_owner: Option<&String>,
        filter_name: Option<&String>,
        filter_date: Option<&String>
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let conn = self.conn.lock().await;

        let mut additional_filters= String::new();
        let mut params = vec![];
        if let Some(filter_state) = filter_states {
            if filter_state.len() == 0 {
                additional_filters.push_str("state = ?1");
                params.push(filter_state[0]);
            } else {
                additional_filters.push_str("state IN (");
                for (i, state) in filter_state.iter().enumerate() {
                    additional_filters.push_str(&format!("?{},", i + 1));
                    params.push(state);
                }
                additional_filters.pop();
                additional_filters.push(')');
            }
        }
        if let Some(filter_queue) = filter_queue {
            if !additional_filters.is_empty() {
                additional_filters.push_str(" AND ");
            }
            additional_filters.push_str(&format!("queue = ?{}", params.len() + 1));
            params.push(filter_queue);
        }
        if let Some(filter_owner) = filter_owner {
            if !additional_filters.is_empty() {
                additional_filters.push_str(" AND ");
            }
            additional_filters.push_str(&format!("owner = ?{}", params.len() + 1));
            params.push(filter_owner);
        }
        if let Some(filter_name) = filter_name {
            if !additional_filters.is_empty() {
                additional_filters.push_str(" AND ");
            }
            additional_filters.push_str(&format!("name = ?{}", params.len() + 1));
            params.push(filter_name);
        }
        // Make sure that the job is before or on the specified date,
        //  OR has not completed (state = R).
        if let Some(filter_date) = filter_date {
            if !additional_filters.is_empty() {
                additional_filters.push_str(" AND ");
            }
            info!("Filtering by date: {filter_date}");
            additional_filters.push_str(&format!("(start_time >= ?{})", params.len() + 1));
            params.push(filter_date);
        }

        // If there were any filters, add the 'WHERE' keyword
        if !additional_filters.is_empty() {
            additional_filters = format!(" WHERE {}", additional_filters);
        }

        let final_query = format!("SELECT * FROM Jobs{}", additional_filters);

        //  ORDER BY start_time DESC
        let mut stmt = conn.prepare(&final_query)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), row.get::<_, String>(2)?),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("start_time".to_string(), row.get::<_, i32>(4)?.to_string()),
                ("queue".to_string(), row.get::<_, String>(5)?),
                ("nodes".to_string(), row.get::<_, String>(6)?),
                ("req_mem".to_string(), row.get::<_, f64>(7)?.to_string()),
                ("req_cpus".to_string(), row.get::<_, i32>(8)?.to_string()),
                ("req_gpus".to_string(), row.get::<_, i32>(9)?.to_string()),
                ("req_walltime".to_string(), row.get::<_, String>(10)?),
                ("req_select".to_string(), row.get::<_, String>(11)?),
                ("mem_efficiency".to_string(), row.get::<_, f64>(12)?.to_string()),
                ("walltime_efficiency".to_string(), row.get::<_, f64>(13)?.to_string()),
                ("cpu_efficiency".to_string(), row.get::<_, f64>(14)?.to_string()),
                ("used_cpu_percent".to_string(), row.get::<_, f64>(15)?.to_string()),
                ("used_mem".to_string(), row.get::<_, f64>(16)?.to_string()),
                ("used_walltime".to_string(), row.get::<_, String>(17)?),
                ("end_time".to_string(), row.get::<_, i32>(18)?.to_string()),
                ("chunks".to_string(), row.get::<_, String>(19)?),
                ("exit_status".to_string(), row.get::<_, String>(20)?),
                ("est_start_time".to_string(), row.get::<_, String>(21)?),
                ("used_cpu_time".to_string(), row.get::<_, String>(22)?),
            ]))
        });

        match rows {
            Ok(rows) => {
                let rows = rows.flatten().collect();
                Ok(rows)
            },
            Err(e) => {
                error!(%e, "Failed to get rows!");
                Err(anyhow!("Failed to get rows! Error: {e:?}"))
            }
        }
    }

    #[tracing::instrument]
    pub async fn _get_group_jobs (
        &self,
        filter_state: Option<&String>,
        filter_queue: Option<&String>,
        filter_owner: Option<&String>,
        filter_name: Option<&String>,
        filter_date: Option<&String>,
        group: &str
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let conn = self.conn.lock().await;
        
        let mut additional_filters= String::new();
        let mut params: Vec<String> = vec![group.to_string()];
        if let Some(filter_state) = filter_state {
            additional_filters.push_str(" AND state = ?2");
            params.push(filter_state.to_owned());
        }
        if let Some(filter_queue) = filter_queue {
            additional_filters.push_str(&format!(" AND queue = ?{}", params.len() + 1));
            params.push(filter_queue.to_owned());
        }
        if let Some(filter_owner) = filter_owner {
            additional_filters.push_str(&format!(" AND owner = ?{}", params.len() + 1));
            params.push(filter_owner.to_owned());
        }
        if let Some(filter_name) = filter_name {
            additional_filters.push_str(&format!(" AND name = ?{}", params.len() + 1));
            params.push(filter_name.to_owned());
        }
        // Make sure that the job is before or on the specified date,
        //  OR has not completed (state = R).
        if let Some(filter_date) = filter_date {
            info!("Filtering by date: {filter_date}");
            additional_filters.push_str(&format!(" AND start_time >= ?{}", params.len() + 1));
            params.push(filter_date.to_owned());
        }

        info!("Filtering by group: {group}");
        info!("Additional filters: '{additional_filters}'");
        info!("Params: {params:?}");

        let mut stmt = conn.prepare(&format!("SELECT * FROM Jobs WHERE owner IN (SELECT user_name FROM UserGroups WHERE group_name = ?1){}", additional_filters))?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), row.get::<_, String>(2)?),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("start_time".to_string(), row.get::<_, i32>(4)?.to_string()),
                ("queue".to_string(), row.get::<_, String>(5)?),
                ("nodes".to_string(), row.get::<_, String>(6)?),
                ("req_mem".to_string(), row.get::<_, f64>(7)?.to_string()),
                ("req_cpus".to_string(), row.get::<_, i32>(8)?.to_string()),
                ("req_gpus".to_string(), row.get::<_, i32>(9)?.to_string()),
                ("req_walltime".to_string(), row.get::<_, String>(10)?),
                ("req_select".to_string(), row.get::<_, String>(11)?),
                ("mem_efficiency".to_string(), row.get::<_, f64>(12)?.to_string()),
                ("walltime_efficiency".to_string(), row.get::<_, f64>(13)?.to_string()),
                ("cpu_efficiency".to_string(), row.get::<_, f64>(14)?.to_string()),
                ("used_cpu_percent".to_string(), row.get::<_, f64>(15)?.to_string()),
                ("used_mem".to_string(), row.get::<_, f64>(16)?.to_string()),
                ("used_walltime".to_string(), row.get::<_, String>(17)?),
                ("end_time".to_string(), row.get::<_, i32>(18)?.to_string()),
                ("chunks".to_string(), row.get::<_, String>(19)?),
                ("exit_status".to_string(), row.get::<_, String>(20)?),
                ("est_start_time".to_string(), row.get::<_, String>(21)?),
                ("used_cpu_time".to_string(), row.get::<_, String>(22)?),
            ]))
        });
    
        match rows {
            Ok(rows) => {
                let ret: Vec<BTreeMap<String, String>> = rows.flatten().collect();
                info!("Returning {} rows!", ret.len());
                Ok(ret)
            },
            Err(e) => {
                error!(%e, "Failed to get rows!");
                Err(anyhow!("Failed to get rows! Error: {e:?}"))
            }
        }
    }

    pub async fn get_job (
        &self,
        pbs_id: i32,
    ) -> Result<BTreeMap<String, String>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare("SELECT * FROM Jobs WHERE pbs_id = ?1")?;
        let row = stmt.query_row([pbs_id], |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), row.get::<_, String>(2)?),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("start_time".to_string(), row.get::<_, i32>(4)?.to_string()),
                ("queue".to_string(), row.get::<_, String>(5)?),
                ("nodes".to_string(), row.get::<_, String>(6)?),
                ("req_mem".to_string(), row.get::<_, f64>(7)?.to_string()),
                ("req_cpus".to_string(), row.get::<_, i32>(8)?.to_string()),
                ("req_gpus".to_string(), row.get::<_, i32>(9)?.to_string()),
                ("req_walltime".to_string(), row.get::<_, String>(10)?),
                ("req_select".to_string(), row.get::<_, String>(11)?),
                ("mem_efficiency".to_string(), row.get::<_, f64>(12)?.to_string()),
                ("walltime_efficiency".to_string(), row.get::<_, f64>(13)?.to_string()),
                ("cpu_efficiency".to_string(), row.get::<_, f64>(14)?.to_string()),
                ("used_cpu_percent".to_string(), row.get::<_, f64>(15)?.to_string()),
                ("used_mem".to_string(), row.get::<_, f64>(16)?.to_string()),
                ("used_walltime".to_string(), row.get::<_, String>(17)?),
                ("end_time".to_string(), row.get::<_, i32>(18)?.to_string()),
                ("chunks".to_string(), row.get::<_, String>(19)?),
                ("exit_status".to_string(), row.get::<_, String>(20)?),
                ("est_start_time".to_string(), row.get::<_, String>(21)?),
                ("used_cpu_time".to_string(), row.get::<_, String>(22)?),
            ]))
        }).context("Failed to get row!")?;
    
        Ok(row)
    }

    pub async fn get_job_stats (
        &self,
        pbs_id: i32,
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let conn = self.conn.lock().await;
        

        let mut stmt = conn.prepare("SELECT * FROM PastStats WHERE pbs_id = ?1")?;
        let rows = stmt.query_map([pbs_id], |row| {
            Ok(BTreeMap::from_iter(vec![
                ("stat_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("pbs_id".to_string(), row.get::<_, i32>(1)?.to_string()),
                ("cpu_percent".to_string(), row.get::<_, f64>(2)?.to_string()),
                ("mem".to_string(), row.get::<_, f64>(3)?.to_string()),
                ("datetime".to_string(), row.get::<_, String>(4)?),
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }

    pub async fn get_users (
        &self,
    ) -> Result<Vec<String>> {
        let conn = self.conn.lock().await;
        

        let mut stmt = conn.prepare("SELECT name FROM Users")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    
        Ok(rows.flatten().collect())
    }
    
    pub async fn _get_user_groups (
        &self,
        username: &str,
    ) -> Result<Vec<String>> {
        let conn = self.conn.lock().await;
        

        let mut stmt = conn.prepare("SELECT group_name FROM UserGroups WHERE user_name = ?1")?;
        let rows = stmt.query_map([username], |row| row.get::<_, String>(0))?;
    
        Ok(rows.flatten().collect())
    }

    pub async fn _is_user_able_to_view_stats (
        &self,
        user: &str,
        pbs_id: i32,
    ) -> Result<bool> {
        let conn = self.conn.lock().await;

        // Firstly, if the user is in the `hpc` group,
        //  they are allowed to view advanced stats for
        //  any job.
        if self._is_user_admin(user).await? {
            return Ok(true);
        }

        // Note that a user is also allowed to view advanced 
        //  stats if the job was created by another user in
        //  the same group as the current user.
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM Jobs WHERE pbs_id = ?1 AND (owner = ?2 OR owner IN (SELECT user_name FROM UserGroups WHERE group_name IN (SELECT group_name FROM UserGroups WHERE user_name = ?2)))")?;
        let count: i32 = stmt.query_row([pbs_id.to_string(), user.to_string()], |row| row.get(0))?;

        Ok(count > 0)
    }

    pub async fn _is_user_in_group (
        &self,
        user: &str,
        group: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare("SELECT COUNT(*) FROM UserGroups WHERE user_name = ?1 AND group_name = ?2")?;
        let count: i32 = stmt.query_row([user, group], |row| row.get(0))?;
    
        Ok(count > 0)
    }

    pub async fn _is_user_admin (
        &self,
        user: &str,
    ) -> Result<bool> {
        self._is_user_in_group(user, "hpc").await
    }

    pub async fn login (
        &self,
        remote_username: &str,
        remote_hostname: &str,
        username: &str,
        password: &str
    ) -> Result<LoginResult> {
        let conn = self.conn.lock().await;

        match verify_login(
            remote_username,
            remote_hostname,
            &username,
            &password
        )
            .await
            .context("Failed to verify login!")?
        {
            true => {
                // Check if the user exists in the DB
                let mut stmt = conn.prepare("SELECT COUNT(*) FROM Users WHERE name = ?1")?;
                let count: i32 = stmt.query_row([username], |row| row.get(0))?;
                drop(stmt);

                // If the user doesn't exist, add them and
                //  populate their groups
                if count == 0 {
                    conn.execute(
                        "INSERT INTO Users (name) VALUES (?1)",
                        [username],
                    )?;
                }

                Ok(LoginResult {
                    success: true,
                    created_new: count == 0,
                })
            },
            false => {
                Ok(LoginResult {
                    success: false,
                    created_new: false,
                })
            }
        }
    }
}