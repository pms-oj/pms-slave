#[macro_use]
extern crate lazy_static;

mod config;
mod constants;
mod container;
mod judge;
mod language;
mod logger;
mod protocol;

pub const CONFIG_FILE: &'static str = "config.toml";

use std::fs::read_to_string;

use log::*;

use config::Config;
use language::Languages;
use protocol::open_protocol;

lazy_static! {
    static ref CONFIG: Config = {
        let s = read_to_string(CONFIG_FILE).expect("Some error occured");
        info!("Loaded PMS slave config file");
        toml::from_str(&s).expect("Some error occured")
    };
    static ref LANGUAGES: Languages = Languages::load().expect("Some error occured");
    static ref MASTER_PASS: Vec<u8> = {
        use sha3::{Digest, Sha3_256};
        let mut hasher = Sha3_256::new();
        hasher.update(CONFIG.master_pass.clone());
        hasher.finalize().to_vec()
    };
}

static LOGGER: logger::StdoutLogger = logger::StdoutLogger;

#[async_std::main]
async fn main() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Debug))
        .ok();
    debug!("{:?}", LANGUAGES.clone());
    info!("pms-slave {}", env!("CARGO_PKG_VERSION"));
    open_protocol().await
}
