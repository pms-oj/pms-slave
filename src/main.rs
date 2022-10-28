#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

mod config;
mod constants;
mod container;
mod judge;
mod language;
mod protocol;
mod timer;

#[cfg(test)]
mod tests;

pub const CONFIG_FILE: &'static str = "config.toml";

use std::fs::read_to_string;

use log::*;

use config::Config;
use constants::LOG_CONFIG_FILE;
use language::Languages;
use protocol::open_protocol;

lazy_static! {
    static ref CONFIG: Config = {
        let s = read_to_string(CONFIG_FILE).expect("Some error occured");
        info!("Loaded PMS slave config file");
        toml::from_str(&s).expect("Some error occured")
    };
    static ref LANGUAGES: Languages = Languages::load().expect("Some error occured");
    static ref MASTER_PASS: Vec<u8> = blake3::hash(CONFIG.host.master_pass.as_bytes()).as_bytes().to_vec();
}

#[async_std::main]
async fn main() {
    log4rs::init_file(LOG_CONFIG_FILE, Default::default()).unwrap();
    info!("pms-slave {}", env!("CARGO_PKG_VERSION"));
    open_protocol().await
}
