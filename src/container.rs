use super::constants::*;
use super::language::Language;
use super::CONFIG;
use std::fs::{read_to_string, File};
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{tempdir, TempDir};

pub fn join_work_dir(file: &str) -> String {
    format!("{}/{}", CONFIG.isolate_work_dir, file)
}

#[derive(Debug)]
pub struct CheckerRun {
    pub checker_lang: Language,
    pub binary_path: PathBuf,
    pub box_dir: TempDir,
}

impl CheckerRun {
    pub fn run(&self) -> RunResult {
        // Clean up
        let _ = Command::new(ISOLATE)
            .arg("--cleanup")
            .output()
            .expect("Failed to run isolate command");
        // Init sandbox
        let _ = Command::new(ISOLATE)
            .arg("--init")
            .output()
            .expect("Failed to run isolate command");
        // Run
        let dir = tempdir().unwrap();
        let log_p = dir.path().join(LOG_FILE_NAME);
        let _ = Command::new(ISOLATE)
            .arg("--run")
            .arg(&format!("-t {}", CHECKER_TIME_LIMIT))
            .arg(&format!("-w {}", CHECKER_TIME_LIMIT))
            .arg(&format!("-m {}", CHECKER_MEM_LIMIT))
            .arg("-s")
            .arg(&format!("--meta={}", log_p.clone().display()))
            .arg(&format!(
                "{} ./{} ./{} ./{}",
                self.checker_lang.parse_exec_cmd(self.binary_path.clone()),
                STDIN_FILE_NAME,
                STDOUT_ORIGIN_FILE_NAME,
                STDOUT_FILE_NAME,
            ))
            .arg(&format!("--dir=box={}", self.box_dir.path().display()))
            .output()
            .expect("Failed to run isolate command");
        let meta = {
            let s = read_to_string(log_p).expect("Some error occured");
            parse_meta(s).expect("Some error occured")
        };
        RunResult { meta }
    }
}

#[derive(Debug)]
pub struct Run {
    pub stdin_path: PathBuf,
    pub stdout_path: PathBuf,
    pub binary_path: PathBuf,
    pub language: Language,
    pub box_dir: TempDir,
    pub time_limit: f64,
    pub mem_limit: u64,
}

impl Run {
    pub fn run(&self) -> RunResult {
        // Clean up
        let _ = Command::new(ISOLATE)
            .arg("--cleanup")
            .output()
            .expect("Failed to run isolate command");
        // Init sandbox
        let _ = Command::new(ISOLATE)
            .arg("--init")
            .output()
            .expect("Failed to run isolate command");
        // Run
        let dir = tempdir().unwrap();
        let log_p = dir.path().join(LOG_FILE_NAME);
        let _ = Command::new(ISOLATE)
            .arg("--run")
            .arg(&format!(
                "-t {}",
                self.time_limit + ((self.language.add_time_limit as f64) * CONVERT_TO_SECONDS)
            ))
            .arg(&format!(
                "-w {}",
                self.time_limit + ((self.language.add_time_limit as f64) * CONVERT_TO_SECONDS)
            ))
            .arg(&format!(
                "-m {}",
                self.mem_limit + self.language.add_mem_limit
            ))
            .arg("-s")
            .arg("-p 1")
            .arg(&format!("--stdin={}", self.stdin_path.display()))
            .arg(&format!("--stdout={}", self.stdout_path.display()))
            .arg(&format!("--meta={}", log_p.clone().display(),))
            .arg(&format!("--dir=box={}", self.box_dir.path().display()))
            .arg(self.language.parse_exec_cmd(self.binary_path.clone()))
            .output()
            .expect("Failed to run isolate command");
        let meta = {
            let s = read_to_string(log_p).expect("Some error occured");
            parse_meta(s).expect("Some error occured")
        };
        RunResult { meta }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RunStatus {
    RuntimeErr,
    DiedOnSignal,
    TimedOut,
    InternalErr,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct RunMeta {
    pub status: Option<RunStatus>,
    pub time: Option<f64>,
    pub time_wall: Option<f64>,
    pub message: Option<String>,
    pub max_rss: Option<i32>,
    pub killed: Option<i32>,
    pub exitsig: Option<i32>,
    pub exitcode: Option<i32>,
    pub csw_voluntary: Option<i32>,
    pub csw_forced: Option<i32>,
    pub cg_mem: Option<u64>,
    pub cg_oom_killed: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct RunResult {
    pub meta: RunMeta,
}

pub fn parse_meta(s: String) -> Option<RunMeta> {
    let mut r: RunMeta = RunMeta {
        status: None,
        time: None,
        time_wall: None,
        message: None,
        max_rss: None,
        killed: None,
        exitsig: None,
        exitcode: None,
        csw_voluntary: None,
        csw_forced: None,
        cg_mem: None,
        cg_oom_killed: None,
    };
    for line in s.split('\n') {
        if !line.is_empty() {
            let v: Vec<&str> = line.split(':').collect();
            match v[0] {
                "time" => {
                    let time: f64 = v[1].parse::<f64>().expect("Some error occured");
                    r.time = Some(time);
                }
                "time-wall" => {
                    let time_wall: f64 = v[1].parse::<f64>().expect("Some error occured");
                    r.time_wall = Some(time_wall);
                }
                "status" => {
                    let status: RunStatus = match v[1] {
                        "TO" => RunStatus::TimedOut,
                        "SG" => RunStatus::DiedOnSignal,
                        "RE" => RunStatus::RuntimeErr,
                        "XX" => RunStatus::InternalErr,
                        _ => RunStatus::Unknown,
                    };
                    r.status = Some(status);
                }
                _ => {}
            }
        }
    }
    if r.status.is_none() {
        None
    } else {
        Some(r)
    }
}
