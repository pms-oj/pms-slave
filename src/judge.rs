use async_std::net::TcpStream;
use bincode::Options;
use judge_protocol::judge::*;
use judge_protocol::packet::*;
use std::path::PathBuf;
use std::pin::Pin;
use tempfile::TempDir;
use uuid::Uuid;

use crate::language::Language;

pub struct OnJudge {
    pub uuid: Uuid,
    pub main_lang: Language,
    pub checker_lang: Language,
    pub manager_lang: Option<Language>,
    pub main_binary: PathBuf,
    pub checker_binary: PathBuf,
    pub object_binary: Option<String>,
    pub time_limit: u64, // in ms
    pub mem_limit: u64,  // in kb
    pub tempdir: TempDir,
}
