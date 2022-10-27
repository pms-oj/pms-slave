pub mod result;

use super::constants::*;
use super::language::Language;
use super::CONFIG;
use std::fs::{read_to_string, File};
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{tempdir, TempDir};

use result::ResultAppes;

#[derive(Debug)]
pub struct CheckerRun {
    pub checker_lang: Language,
    pub temp_path: PathBuf,
    pub box_dir: TempDir,
}

impl CheckerRun {
    pub fn run(&self) -> CheckerResult {
        // Clean up
        let _ = Command::new(ISOLATE)
            .arg("--cg")
            .arg("--cleanup")
            .output()
            .expect("Failed to run isolate command");
        // Init sandbox
        let _ = Command::new(ISOLATE)
            .arg("--init")
            .arg("--cg")
            .output()
            .expect("Failed to run isolate command");
        // Run
        let dir = tempdir().unwrap();
        let log_p = dir.path().join(LOG_FILE_NAME);
        let result_p = self.box_dir.path().join(RESULT_FILE_NAME);
        std::fs::copy(CHECKER_SH, self.box_dir.path().join(CHECKER_SH)).ok();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(self.box_dir.path(), std::fs::Permissions::from_mode(0o777)).ok();
        std::fs::set_permissions(
            self.box_dir.path().join(CHECKER_SH),
            std::fs::Permissions::from_mode(0o777),
        )
        .ok();
        std::fs::set_permissions(
            self.temp_path.join(CHECKER_NAME),
            std::fs::Permissions::from_mode(0o777),
        )
        .ok();
        let out = Command::new(ISOLATE)
            .arg("--run")
            .arg("--cg")
            .arg(&format!(
                "-t {}",
                CHECKER_TIME_LIMIT
                    + ((self.checker_lang.add_time_limit as f64) * CONVERT_TO_SECONDS)
            ))
            .arg(&format!(
                "-w {}",
                CHECKER_TIME_LIMIT
                    + ((self.checker_lang.add_time_limit as f64) * CONVERT_TO_SECONDS)
            ))
            .arg(&format!(
                "-m {}",
                CHECKER_MEM_LIMIT + self.checker_lang.add_mem_limit
            ))
            .arg(&format!(
                "--cg-mem={}",
                CHECKER_MEM_LIMIT + self.checker_lang.add_mem_limit
            ))
            .arg("-p 2")
            .arg("-s")
            .arg(&format!("--meta={}", log_p.clone().display()))
            .arg(&format!("--dir=temp={}", self.temp_path.display()))
            .arg(&format!("--dir=box={}:rw", self.box_dir.path().display()))
            .arg(BASH)
            .arg(CHECKER_SH)
            .output()
            .expect("Failed to run isolate command");
        debug!(
            "(Checker) stderr: {}",
            String::from_utf8(out.stderr).unwrap()
        );
        debug!(
            "(Checker) stdout: {}",
            String::from_utf8(out.stdout.clone()).unwrap()
        );
        let result_str = read_to_string(result_p).expect("Failed to read a result file");
        debug!("(Checker) {}: {}", RESULT_FILE_NAME, result_str.clone());
        let result = toml::from_str::<ResultAppes>(&result_str).expect("Failed to parse a result file");
        let meta = {
            let s = read_to_string(log_p).expect("Failed to read a log file");
            parse_meta(s).expect("Failed to parse a log file")
        };
        CheckerResult {
            score: result
                .points
                .map(|sc| sc.parse::<f64>().expect("Failed to parse a score")),
            meta,
        }
    }
}

#[derive(Debug)]
pub struct Runv2 {
    pub temp_path: PathBuf,
    pub object_path: String,
    pub main_lang: Language,
    pub manager_lang: Language,
    pub box_dir: TempDir,
    pub time_limit: f64,
    pub mem_limit: u64,
}

impl Runv2 {
    pub fn run(&self) -> RunResult {
        // Clean up
        let _ = Command::new(ISOLATE)
            .arg("--cg")
            .arg("--cleanup")
            .output()
            .expect("Failed to run isolate command");
        // Init sandbox
        let _ = Command::new(ISOLATE)
            .arg("--init")
            .arg("--cg")
            .output()
            .expect("Failed to run isolate command");
        // Run
        std::fs::copy(RUN_JUDGE_SH, self.box_dir.path().join(RUN_JUDGE_SH)).ok();
        let exec_sh = self.box_dir.path().join(EXEC_SH);
        let exec_man_sh = self.box_dir.path().join(EXEC_MAN_SH);
        let mut exec_f = File::create(exec_sh.clone()).unwrap();
        let mut exec_man_f = File::create(exec_man_sh.clone()).unwrap();
        exec_f
            .write_all(
                self.main_lang
                    .parse_exec_sh(PathBuf::from(&format!(
                        "/temp/{}/{}",
                        GRADERS_PATH, self.object_path
                    )))
                    .as_bytes(),
            )
            .ok();
        exec_man_f
            .write_all(
                self.manager_lang
                    .parse_exec_sh(PathBuf::from(&format!("/temp/{}", MANAGER_NAME)))
                    .as_bytes(),
            )
            .ok();
        exec_f.flush().ok();
        exec_man_f.flush().ok();
        drop(exec_f);
        drop(exec_man_f);
        let dir = tempdir().unwrap();
        let log_p = dir.path().join(LOG_FILE_NAME);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            self.box_dir.path().to_path_buf(),
            std::fs::Permissions::from_mode(0o777),
        )
        .ok();
        std::fs::set_permissions(
            exec_sh.to_path_buf(),
            std::fs::Permissions::from_mode(0o777),
        )
        .ok();
        std::fs::set_permissions(
            exec_man_sh.to_path_buf(),
            std::fs::Permissions::from_mode(0o777),
        )
        .ok();
        let out = Command::new(ISOLATE)
            .arg("--run")
            .arg("--cg")
            .arg(&format!(
                "-t {}",
                self.time_limit + ((self.main_lang.add_time_limit as f64) * CONVERT_TO_SECONDS)
            ))
            .arg(&format!(
                "-w {}",
                self.time_limit + ((self.main_lang.add_time_limit as f64) * CONVERT_TO_SECONDS)
            ))
            .arg(&format!(
                "-m {}",
                self.mem_limit + self.main_lang.add_mem_limit
            ))
            .arg(&format!(
                "--cg-mem={}",
                self.mem_limit + self.main_lang.add_mem_limit
            ))
            .arg("-p 5")
            .arg("-s")
            .arg(&format!("--stdin=/temp/{}", STDIN_FILE_NAME))
            .arg(&format!("--meta={}", log_p.clone().display(),))
            .arg(&format!("--dir=temp={}", self.temp_path.display()))
            .arg(&format!("--dir=box={}:rw", self.box_dir.path().display()))
            .arg(BASH)
            .arg(&format!("/box/{}", RUN_JUDGE_SH))
            .output()
            .expect("Failed to run isolate command");
        debug!("(Runv2) stderr: {}", String::from_utf8(out.stderr).unwrap());
        debug!(
            "(Runv2) stdout: {}",
            String::from_utf8(out.stdout.clone()).unwrap()
        );
        debug!(
            "(Runv2) manager.err: {}",
            std::fs::read_to_string(self.box_dir.path().join("manager.err")).unwrap()
        );
        debug!(
            "(Runv2) manager.ret: {}",
            std::fs::read_to_string(self.box_dir.path().join("manager.ret")).unwrap()
        );
        debug!(
            "(Runv2) grader.err: {}",
            std::fs::read_to_string(self.box_dir.path().join("grader.err")).unwrap()
        );
        debug!(
            "(Runv2) grader.ret: {}",
            std::fs::read_to_string(self.box_dir.path().join("grader.ret")).unwrap()
        );
        let mut stdout_f = File::create(self.temp_path.join(STDOUT_FILE_NAME)).unwrap();
        stdout_f.write_all(&out.stdout).ok();
        stdout_f.flush().ok();
        let meta = {
            let s = read_to_string(log_p).expect("Some error occured");
            parse_meta(s).expect("Some error occured")
        };
        RunResult { meta }
    }
}

#[derive(Debug)]
pub struct Run {
    pub temp_path: PathBuf,
    pub language: Language,
    pub box_dir: TempDir,
    pub time_limit: f64,
    pub mem_limit: u64,
}

impl Run {
    pub fn run(&self) -> RunResult {
        // Clean up
        let _ = Command::new(ISOLATE)
            .arg("--cg")
            .arg("--cleanup")
            .output()
            .expect("Failed to run isolate command");
        // Init sandbox
        let _ = Command::new(ISOLATE)
            .arg("--init")
            .arg("--cg")
            .output()
            .expect("Failed to run isolate command");
        // Run
        let dir = tempdir().unwrap();
        let log_p = dir.path().join(LOG_FILE_NAME);
        let out = Command::new(ISOLATE)
            .arg("--run")
            .arg("--cg")
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
            .arg(&format!(
                "--cg-mem={}",
                self.mem_limit + self.language.add_mem_limit
            ))
            .arg("-s")
            .arg(&format!("--stdin=/temp/{}", STDIN_FILE_NAME))
            .arg(&format!("--stdout=/temp/{}", STDOUT_FILE_NAME))
            .arg(&format!("--meta={}", log_p.clone().display(),))
            .arg(&format!("--dir=temp={}:rw", self.temp_path.display()))
            .arg(&format!("--dir=box={}", self.box_dir.path().display()))
            .arg(
                self.language
                    .parse_exec_cmd(PathBuf::from(&format!("/temp/{}", BINARY_NAME))),
            )
            .output()
            .expect("Failed to run isolate command");
        debug!("(Run) stderr: {}", String::from_utf8(out.stderr).unwrap());
        debug!("(Run) stdout: {}", String::from_utf8(out.stdout).unwrap());
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

#[derive(Clone, Debug)]
pub struct CheckerResult {
    pub score: Option<f64>,
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
                    let time: f64 = v[1].parse::<f64>().expect("Some error occurred");
                    r.time = Some(time);
                }
                "time-wall" => {
                    let time_wall: f64 = v[1].parse::<f64>().expect("Some error occurred");
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
                "cg-mem" => {
                    let cg_mem: u64 = v[1].parse::<u64>().expect("Some error occurred");
                    r.cg_mem = Some(cg_mem);
                }
                "exitcode" => {
                    let code: i32 = v[1].parse::<i32>().expect("Some error occurred");
                    r.exitcode = Some(code);
                }
                "exitsig" => {
                    let sig: i32 = v[1].parse::<i32>().expect("Some error occurred");
                    r.exitsig = Some(sig);
                }
                _ => {}
            }
        }
    }
    Some(r)
}
