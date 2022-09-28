#[macro_use]
extern crate lazy_static;

mod config;
mod container;
mod language;
mod protocol;

pub const CONFIG_FILE: &'static str = "config.toml";

use std::fs::read_to_string;

use log::{info, debug};

use config::Config;
use language::Languages;
use protocol::open_protocol;

lazy_static! {
    static ref CONFIG: Config = {
        let s = read_to_string(CONFIG_FILE).expect("Some error occured");
        info!("[PMS-slave] Loaded PMS slave config file");
        toml::from_str(&s).expect("Some error occured")
    };
    static ref LANGUAGES: Languages = Languages::load().expect("Some error occured");
}

#[async_std::main]
async fn main() {
    open_protocol().await
}
