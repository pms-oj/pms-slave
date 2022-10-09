#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;

mod config;
mod constants;
mod container;
mod judge;
mod language;
mod logger;
mod protocol;
mod timer;

pub const CONFIG_FILE: &'static str = "config.toml";

use std::fs::read_to_string;

use log::*;

use config::Config;
use fast_log::appender::LogAppender;
use language::Languages;
use logger::*;
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
        hasher.update(CONFIG.host.master_pass.clone());
        hasher.finalize().to_vec()
    };
}

#[async_std::main]
async fn main() {
    match CONFIG.logging.method {
        Method::Stdout => {
            fast_log::init(
                fast_log::Config::new()
                    .level(CONFIG.logging.max_level.unwrap().to_level_filter())
                    .custom(Logger {})
                    .console(),
            )
            .unwrap();
        }
        Method::File => {
            fast_log::init(
                fast_log::Config::new()
                    .level(CONFIG.logging.max_level.unwrap().to_level_filter())
                    .custom(Logger {})
                    .file("log/pms-slave.log"),
            )
            .unwrap();
        }
        _ => {}
    }
    info!("pms-slave {}", env!("CARGO_PKG_VERSION"));
    open_protocol().await
}
