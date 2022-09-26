use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub master: SocketAddr,
    pub master_pass: String,
    pub isolate_flag: String,
    pub isolate_work_dir: String,
}
