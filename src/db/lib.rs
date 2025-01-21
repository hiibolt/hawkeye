use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use colored::Colorize;

#[derive(Debug)]
pub struct DB {
    conn: Connection,
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
                FOREIGN KEY (pbs_id) REFERENCES Jobs(pbs_id)
            )",
            [],
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO Groups (name) VALUES ('admin')",
            [],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO Groups (name) VALUES ('zwlab')",
            [],
        )?;
        
        Ok(Self {
            conn,
        })
    }
    pub fn insert_job ( &mut self, job: &BTreeMap<&str, String> ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO Users (name) VALUES (?1)",
            [&job["Job_Name"]],
        )?;
        
        // Add the job
        self.conn.execute(
            "INSERT OR REPLACE INTO Jobs (pbs_id, name, owner, state, stime, queue, nodes, req_mem, req_cpus, req_gpus, req_walltime, req_select, mem_efficiency, walltime_efficiency) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
            ],
        )?;
        
        // Add the latest stats
        self.conn.execute(
            "INSERT INTO PastStats (pbs_id, cpu_percent, mem) VALUES (?1, ?2, ?3)",
            params![
                job["job_id"],
                job["resources_used.cpupercent"],
                job["resources_used.mem"],
            ],
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
    
                self.conn.execute(
                    "UPDATE Jobs SET state = 'S' WHERE pbs_id = ?1",
                    [pbs_id],
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
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }

    pub fn get_job_stats(
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
            ]))
        }).context("Failed to get rows!")?;
    
        Ok(rows.flatten().collect())
    }
}