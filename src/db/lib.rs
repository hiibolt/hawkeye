use std::{collections::{BTreeMap, HashSet}, time::{SystemTime, UNIX_EPOCH}};
use chrono::{DateTime, Utc};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use colored::Colorize;

use super::super::remote::auth::verify_login;

#[derive(Debug)]
pub struct DB {
    conn: Connection,
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
                stime TEXT NOT NULL,
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
                end_time TEXT,
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
            conn,
        })
    }
    pub fn insert_job ( &mut self, job: &BTreeMap<&str, String> ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO Users (name) VALUES (?1)",
            [&job["Job_Owner"]],
        )?;
        
        // Add the job
        self.conn.execute(
            "INSERT OR REPLACE INTO Jobs (pbs_id, name, owner, state, stime, queue, nodes, req_mem, req_cpus, req_gpus, req_walltime, req_select, mem_efficiency, walltime_efficiency, cpu_efficiency, end_time) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                job["job_id"],
                job["Job_Name"],
                job["Job_Owner"],
                job["job_state"],
                job["stime"],
                job["queue"],
                job["Nodes"],
                job["Resource_List.mem"],
                job["Resource_List.ncpus"],
                job.get("Resource_List.ngpus").unwrap_or(&String::from("0")),
                job["Resource_List.walltime"],
                job["Resource_List.select"],
                job["mem_efficiency"],
                job["walltime_efficiency"],
                job["cpu_efficiency"],
                job.get("end_time").unwrap_or(&String::from("not_ended")),
            ],
        )?;
        
        // Add the latest stats
        // Get the current system time
        let now = SystemTime::now();
        let duration_since_epoch = now.duration_since(UNIX_EPOCH)
            .context("Time went backwards")?;
        let datetime = DateTime::<Utc>::from(UNIX_EPOCH + duration_since_epoch);
        let formatted_datetime = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn.execute(
            "INSERT INTO PastStats (pbs_id, cpu_percent, mem, datetime) VALUES (?1, ?2, ?3, ?4)",
            params![
                job["job_id"],
                job["resources_used.cpupercent"],
                job["resources_used.mem"],
                formatted_datetime
            ],
        )?;

        Ok(())
    }

    pub fn insert_user_group (
        &mut self,
        user: &str,
        group: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO Groups (name) VALUES (?1)",
            [group],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO UserGroups (user_name, group_name) VALUES (?1, ?2)",
            [user, group],
        )?;

        Ok(())
    }
    
    /// Update all jobs in state 'R' that are *not* in the current list of active jobs.
    /// We mark them as 'S' in the database (i.e., 'stopped' or 'completed').
    pub fn mark_completed_jobs(
        &mut self,
        active_jobs: &[BTreeMap<&str, String>],
    ) -> Result<()> {
        // Build a set of IDs for *currently active* jobs
        let active_ids: HashSet<_> = active_jobs
            .iter()
            .flat_map(|job| job["job_id"].parse::<i32>())
            .collect();
        
        // Find all jobs that are in state 'R' in our local DB
        let mut stmt = self.conn.prepare("SELECT pbs_id FROM Jobs WHERE state = 'R'")?;
        let rows = stmt.query_map([], |row| row.get::<_, i32>(0))?;
    
        // For each job in state 'R', check if it's still active
        for row_result in rows {
            let pbs_id = row_result?;
            // If a job's ID is *not* in the active set, we assume it completed
            if !active_ids.contains(&pbs_id) {
                eprintln!("{}", format!("[ Marking job {} as completed... ]", pbs_id).green());

                let now = SystemTime::now();
                let duration_since_epoch = now.duration_since(UNIX_EPOCH)
                    .context("Time went backwards")?;
                let datetime = DateTime::<Utc>::from(UNIX_EPOCH + duration_since_epoch);
                let formatted_datetime = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

                self.conn.execute(
                    "UPDATE Jobs SET state = 'S' WHERE pbs_id = ?1",
                    [pbs_id],
                )?;

                self.conn.execute(
                    "UPDATE Jobs SET end_time = ?1 WHERE pbs_id = ?2",
                    [formatted_datetime, pbs_id.to_string()],
                )?;
            }
        }
    
        Ok(())
    }

    pub fn get_user_jobs(
        &mut self,
        username: &str,
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let mut stmt = self.conn.prepare("SELECT * FROM Jobs WHERE owner = ?1")?;
        let rows = stmt.query_map([username], |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), row.get::<_, String>(2)?),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("stime".to_string(), row.get::<_, String>(4)?),
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
                ("end_time".to_string(), row.get::<_, String>(15)?),
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }

    pub fn get_all_running_jobs (
        &mut self,
        censor: bool
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let mut stmt = self.conn.prepare("SELECT * FROM Jobs WHERE state = 'R'")?;
        let rows = stmt.query_map([], |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), if censor { "REDACTED".to_string() } else { row.get::<_, String>(2)? }),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("stime".to_string(), row.get::<_, String>(4)?),
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
                ("end_time".to_string(), row.get::<_, String>(15)?),
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }

    pub fn get_group_jobs (
        &mut self,
        group: &str,
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let mut stmt = self.conn.prepare("SELECT * FROM Jobs WHERE owner IN (SELECT user_name FROM UserGroups WHERE group_name = ?1)")?;
        let rows = stmt.query_map([group], |row| {
            Ok(BTreeMap::from_iter(vec![
                ("pbs_id".to_string(), row.get::<_, i32>(0)?.to_string()),
                ("name".to_string(), row.get::<_, String>(1)?),
                ("owner".to_string(), row.get::<_, String>(2)?),
                ("state".to_string(), row.get::<_, String>(3)?),
                ("stime".to_string(), row.get::<_, String>(4)?),
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
                ("end_time".to_string(), row.get::<_, String>(15)?),
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }

    pub fn get_job_stats (
        &mut self,
        pbs_id: i32,
    ) -> Result<Vec<BTreeMap<String, String>>> {
        let mut stmt = self.conn.prepare("SELECT * FROM PastStats WHERE pbs_id = ?1")?;
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

    pub fn get_users (
        &mut self,
    ) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM Users")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    
        Ok(rows.flatten().collect())
    }
    
    pub fn get_user_groups (
        &mut self,
        username: &str,
    ) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT group_name FROM UserGroups WHERE user_name = ?1")?;
        let rows = stmt.query_map([username], |row| row.get::<_, String>(0))?;
    
        Ok(rows.flatten().collect())
    }

    pub fn is_user_able_to_view_stats (
        &mut self,
        user: &str,
        pbs_id: i32,
    ) -> Result<bool> {
        // Firstly, if the user is in the `hpc` group,
        //  they are allowed to view advanced stats for
        //  any job.
        if self.is_user_admin(user)? {
            return Ok(true);
        }

        // Note that a user is also allowed to view advanced 
        //  stats if the job was created by another user in
        //  the same group as the current user.
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM Jobs WHERE pbs_id = ?1 AND (owner = ?2 OR owner IN (SELECT user_name FROM UserGroups WHERE group_name IN (SELECT group_name FROM UserGroups WHERE user_name = ?2)))")?;
        let count: i32 = stmt.query_row([pbs_id.to_string(), user.to_string()], |row| row.get(0))?;

        Ok(count > 0)
    }

    pub fn is_user_in_group (
        &mut self,
        user: &str,
        group: &str,
    ) -> Result<bool> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM UserGroups WHERE user_name = ?1 AND group_name = ?2")?;
        let count: i32 = stmt.query_row([user, group], |row| row.get(0))?;
    
        Ok(count > 0)
    }

    pub fn is_user_admin (
        &mut self,
        user: &str,
    ) -> Result<bool> {
        self.is_user_in_group(user, "hpc")
    }

    pub async fn login (
        &mut self,
        remote_username: &str,
        remote_hostname: &str,
        username: &str,
        password: &str
    ) -> Result<LoginResult> {

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
                let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM Users WHERE name = ?1")?;
                let count: i32 = stmt.query_row([username], |row| row.get(0))?;
                drop(stmt);

                // If the user doesn't exist, add them and
                //  populate their groups
                if count == 0 {
                    self.conn.execute(
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