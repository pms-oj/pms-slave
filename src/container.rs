const ISOLATE: &'static str = "isolate";
const LOG_FILE_NAME: &'static str = "main.log";
const STDIN_FILE_NAME: &'static str = "stdin.in";
const STDOUT_FILE_NAME: &'static str = "stdout.out";

use super::language::Language;
use super::CONFIG;
use std::fs::{read_to_string, File};
use std::io::prelude::*;
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Run {
    stdin: String,
    language: Language,
    binary_path: String,
    time_limit: f64,
    mem_limit: f64,
}

impl Run {
    fn run(&self) -> RunResult {
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
        let mut stdin = File::create(&format!("{}/{}", CONFIG.isolate_work_dir, STDIN_FILE_NAME))
            .expect("Some error occured");
        stdin.write_all(self.stdin.clone().as_bytes()).ok();
        // Run
        let _ = Command::new(ISOLATE)
            .arg("--run")
            .arg(&format!(
                "-t {}",
                self.time_limit + self.language.add_time_limit
            ))
            .arg(&format!(
                "-w {}",
                self.time_limit + self.language.add_time_limit
            ))
            .arg(&format!(
                "-m {}",
                self.mem_limit + self.language.add_mem_limit
            ))
            .arg("-s")
            .arg(&format!("--stdin=./{}", STDIN_FILE_NAME))
            .arg(&format!("--stdout=./{}", STDOUT_FILE_NAME))
            .arg(&format!("--meta=./{}", LOG_FILE_NAME))
            .arg(&format!("--dir=box={}", CONFIG.isolate_work_dir))
            .arg(&format!(
                "{} {} {}",
                self.language.exec, self.language.args, self.binary_path
            ))
            .output()
            .expect("Failed to run isolate command");
        let stdout: String =
            read_to_string(&format!("{}/{}", CONFIG.isolate_work_dir, STDOUT_FILE_NAME))
                .expect("Some error occured");
        let meta = {
            let s = read_to_string(&format!("{}/{}", CONFIG.isolate_work_dir, LOG_FILE_NAME))
                .expect("Some error occured");
            parse_meta(s).expect("Some error occured")
        };
        RunResult { meta, stdout }
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
    status: Option<RunStatus>,
    time: Option<f64>,
    time_wall: Option<f64>,
    message: Option<String>,
    max_rss: Option<i32>,
    killed: Option<i32>,
    exitsig: Option<i32>,
    exitcode: Option<i32>,
    csw_voluntary: Option<i32>,
    csw_forced: Option<i32>,
    cg_mem: Option<i32>,
    cg_oom_killed: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct RunResult {
    meta: RunMeta,
    stdout: String,
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
