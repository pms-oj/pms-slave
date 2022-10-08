use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub master: SocketAddr,
    pub master_pass: String,
}
