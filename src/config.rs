use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use crate::logger::*;

#[derive(Deserialize, Serialize, Debug)]
pub struct Host {
    pub master: SocketAddr,
    pub master_pass: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Logging {
    pub method: Method,
    pub max_level: Option<MaxLevel>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub host: Host,
    pub logging: Logging,
}
