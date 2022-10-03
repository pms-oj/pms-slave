use async_std::net::TcpStream;
use bincode::Options;
use judge_protocol::judge::*;
use judge_protocol::packet::*;
use std::path::PathBuf;
use std::pin::Pin;
use uuid::Uuid;

use crate::language::Language;

pub struct OnJudge {
    pub uuid: Uuid,
    pub main_lang: Language,
    pub checker_lang: Language,
    pub main_binary: PathBuf,
    pub checker_binary: PathBuf,
    pub time_limit: f64,
    pub mem_limit: f64,
}
