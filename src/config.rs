use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Host {
    pub master: String,
    pub master_pass: String,
}

impl Host {
    fn validate(&self) {
        // TODO
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Redis {
    pub enabled: bool,
    pub redis: Option<String>,
}

impl Redis {
    fn validate(&self) {
        if self.enabled {
            if self.redis.is_none() {
                panic!("`redis` feature is enabled. but you didn't provide redis address.");
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub host: Host,
    pub redis: Redis,
}

impl Config {
    pub fn validate(&self) {
        self.host.validate();
        self.redis.validate();
    }
}