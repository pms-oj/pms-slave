use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Host {
    pub master: String,
    pub master_pass: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub host: Host,
}
