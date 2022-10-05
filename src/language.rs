use serde::{Deserialize, Serialize};
use uuid::Uuid;

use std::collections::HashMap;
use std::fs::{read_dir, read_to_string};
use std::io::{self, prelude::*};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;
use tinytemplate::TinyTemplate;

const LANGUAGES_DIR: &'static str = "langs";

#[derive(Deserialize, Debug, Clone)]
pub struct Language {
    pub uuid: Uuid,
    pub name: String, // Display name
    pub version: String,
    pub exec_cmd: String,
    pub compile_cmd: String,
    pub add_mem_limit: u64,
    pub add_time_limit: u64,
}

#[derive(Serialize)]
pub struct ExecCmd {
    file: PathBuf,
}

#[derive(Serialize)]
pub struct CompileCmd {
    infile: PathBuf,
    outfile: PathBuf,
}

#[derive(Clone, Debug)]
pub enum CompileResult {
    Success(String),
    Error(String),
}

impl Language {
    pub fn parse_exec_cmd(&self, binary_path: PathBuf) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("exec", &self.exec_cmd).ok();
        let exec = ExecCmd { file: binary_path };
        tt.render("exec", &exec).unwrap()
    }

    pub fn parse_compile_cmd(&self, infile: PathBuf, outfile: PathBuf) -> String {
        let mut tt = TinyTemplate::new();
        tt.add_template("compile", &self.compile_cmd).ok();
        let compile = CompileCmd { infile, outfile };
        tt.render("compile", &compile).unwrap()
    }

    pub fn compile(&self, code: Vec<u8>, outfile: PathBuf) -> CompileResult {
        let mut tempfile = NamedTempFile::new().unwrap();
        tempfile.write_all(&code).ok();
        let cmd =
            Command::new(self.parse_compile_cmd(tempfile.into_temp_path().to_path_buf(), outfile))
                .output()
                .expect("Failed to compile");
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
        let binding = format!("./{}", LANGUAGES_DIR).clone();
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
