use serde::{Deserialize, Serialize};
use uuid::Uuid;

use std::collections::HashMap;
use std::fs::{read_dir, read_to_string};
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};
use std::process::Command;

use async_std::fs::File;
use async_std::io::prelude::*;

use tempfile::NamedTempFile;

use tinytemplate::TinyTemplate;

use redis::{Client, AsyncCommands};

use crate::constants::*;
use crate::CONFIG;

#[derive(Deserialize, Debug, Clone)]
pub struct Language {
    pub uuid: Uuid,
    pub name: String, // Display name
    pub version: String,
    pub exec_cmd: String,
    pub compile_exec: String,
    pub compile_args: String,
    pub entry_source: String,
    pub add_mem_limit: u64,
    pub add_time_limit: u64,
}

#[derive(Serialize)]
pub struct ExecCmd {
    file: PathBuf,
}

#[derive(Serialize)]
pub struct ExecSh {
    language_command: String,
}

#[derive(Serialize)]
pub struct CompileCmd {
    infile: PathBuf,
    outfile: PathBuf,
}

#[derive(Serialize)]
pub struct MakeCmd {
    threads: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum CompileResult {
    Success(String),
    Error(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheGraderCompile {
    result: CompileResult,
    raw: Vec<u8>,
}

pub fn parse_make_args() -> String {
    let mut tt = TinyTemplate::new();
    tt.add_template("make", MAKE_ARGS).ok();
    let make = MakeCmd {
        threads: num_cpus::get(),
    };
    tt.render("make", &make).unwrap()
}

pub async fn compile_with_graders(
    grader_hash: &[u8; 32],
    grader_path: PathBuf,
    code: Vec<u8>,
    object_rpath: String,
    code_rpath: String,
) -> CompileResult {
    if CONFIG.redis.enabled {
        let code_hash = *blake3::hash(&code).as_bytes();
        let client = redis::Client::open(CONFIG.redis.redis.as_ref().unwrap().clone()).expect(&format!("Cannot make connection to redis server: {}", CONFIG.redis.redis.as_ref().unwrap().clone()));
        let mut con = client.get_async_connection().await.expect(&format!("Cannot establish connection to redis server: {}", CONFIG.redis.redis.as_ref().unwrap().clone()));
        if let Ok(cache) = con.get(&[&grader_hash, &code_hash]).await {
            let cache: Vec<u8> = cache;
            if let Ok(cache_object) = bson::from_slice::<CacheGraderCompile>(&cache) {
                let mut f = File::create(grader_path.clone().join(object_rpath)).await.unwrap();
                f.write_all(&cache_object.raw).await.unwrap();
                f.sync_all().await.unwrap();
                return cache_object.result;
            } else {
                warn!("Maybe cache is corrupted?");
            }
        }
    }
    // TODO: make it works as asynchronous
    let path = grader_path.clone().join(code_rpath);
    let mut tempfile = File::create(path.clone()).await.unwrap();
    tempfile.write_all(&code).await.unwrap();
    tempfile.sync_all().await.unwrap();
    let cmd = Command::new(MAKE)
        .current_dir(grader_path.clone())
        .args(parse_make_args().split_whitespace())
        .output()
        .expect("Failed to compile");
    let res = if cmd.status.success() {
        CompileResult::Success(String::from_utf8(cmd.stdout).unwrap())
    } else {
        CompileResult::Error(String::from_utf8(cmd.stderr).unwrap())
    };
    if CONFIG.redis.enabled {
        let object_data: Vec<u8> = {
            if let Ok(mut object_f) = File::open(grader_path.join(object_rpath)).await {
                let mut buf = vec![];
                object_f.read_to_end(&mut buf).await.unwrap();
                buf
            } else {
                vec![]
            }
        };
        let cache = CacheGraderCompile {
            result: res.clone(),
            raw: object_data,
        };
        let code_hash: [u8;32] = *blake3::hash(&code).as_bytes();
        let client = redis::Client::open(CONFIG.redis.redis.as_ref().unwrap().clone()).expect(&format!("Cannot make connection to redis server: {}", CONFIG.redis.redis.as_ref().unwrap().clone()));
        let mut con = client.get_async_connection().await.expect(&format!("Cannot establish connection to redis server: {}", CONFIG.redis.redis.as_ref().unwrap().clone()));
        let _: () = con.set(&[&grader_hash, &code_hash], bson::to_vec(&cache).unwrap()).await.unwrap();
    }
    res
}

impl Language {
    pub fn parse_exec_cmd(&self, binary_path: PathBuf) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("exec", &self.exec_cmd).ok();
        let exec = ExecCmd { file: binary_path };
        tt.render("exec", &exec).unwrap()
    }

    pub fn parse_exec_sh(&self, binary_path: PathBuf) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("sh", include_str!("../assets/scripts/exec.template.sh"))
            .ok();
        let sh = ExecSh {
            language_command: self.parse_exec_cmd(binary_path),
        };
        tt.render("sh", &sh).unwrap()
    }

    pub fn parse_compile_args(&self, infile: PathBuf, outfile: PathBuf) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("compile", &self.compile_args).ok();
        let compile = CompileCmd { infile, outfile };
        tt.render("compile", &compile).unwrap()
    }

    pub async fn compile(&self, code: Vec<u8>, outfile: PathBuf) -> CompileResult {
        // TODO: make it works as asynchronous
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(self.entry_source.clone());
        let mut tempfile = std::fs::File::create(path.clone()).unwrap();
        tempfile.write_all(&code).ok();
        tempfile.flush().ok();
        let cmd = Command::new(&self.compile_exec)
            .args(
                self.parse_compile_args(path.to_path_buf(), outfile.clone())
                    .split_whitespace(),
            )
            .output()
            .expect("Failed to compile");
        trace!("{:?}", outfile.clone());
        if cmd.status.success() {
            CompileResult::Success(String::from_utf8(cmd.stdout).unwrap())
        } else {
            CompileResult::Error(String::from_utf8(cmd.stderr).unwrap())
        }
    }
}

#[derive(Debug, Clone)]
pub struct Languages {
    langs: HashMap<Uuid, Language>,
}

impl Languages {
    pub fn load() -> io::Result<Self> {
        let binding = format!("./{}", LANGUAGES_PATH).clone();
        let dir = Path::new(&binding);
        assert_eq!(dir.is_dir(), true);
        let mut map = HashMap::new();
        for entry in read_dir(dir)? {
            let entry = entry?;
            if let Ok(file_t) = entry.file_type() {
                if file_t.is_file() {
                    let path = entry.path();
                    let s = read_to_string(path).expect("Some error occured");
                    if let Ok(lang) = toml::from_str::<Language>(&s) {
                        map.insert(lang.uuid.clone(), lang.clone());
                    }
                }
            }
        }
        Ok(Self { langs: map })
    }

    pub fn get(&self, id: Uuid) -> Option<&Language> {
        self.langs.get(&id)
    }
}
