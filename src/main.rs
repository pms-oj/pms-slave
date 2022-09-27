#[macro_use]
extern crate lazy_static;

mod config;
mod container;
mod language;

use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::task::{spawn};

use bincode::Options;

use judge_protocol::packet::*;
use k256::ecdh::EphemeralSecret;
use k256::ecdh::SharedSecret;
use k256::PublicKey;
use rand::thread_rng;
use std::fs::read_to_string;

use async_std::sync::*;

use config::Config;
use language::Languages;
use std::pin::Pin;

use log::{debug, info};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const CONFIG_FILE: &'static str = "config.toml";

lazy_static! {
    static ref CONFIG: Config = {
        let s = read_to_string(CONFIG_FILE).expect("Some error occured");
        info!("[PMS-slave] Loaded PMS slave config file");
        toml::from_str(&s).expect("Some error occured")
    };
    static ref LANGUAGES: Languages = Languages::load().expect("Some error occured");
}

struct State {
    key: Arc<EphemeralSecret>,
    shared: Arc<Mutex<Option<SharedSecret>>>,
}

impl State {
    async fn handle_command(&mut self, _stream: &mut TcpStream, packet: Packet) {
        match packet.heady.header.command {
            Command::Handshake => {
                if let Ok(server_pubkey) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<PublicKey>(&packet.heady.body)
                {
                    self.shared =
                        Arc::new(Mutex::new(Some(self.key.diffie_hellman(&server_pubkey))));
                } else {
                    debug!("[PMS-slave] An error occurred");
                }
            },
            Command::ReqVerifyToken => {
                
            },
            _ => {
                debug!("[PMS-slave] An unknown command has received");
                // Unknown
            }
        }
    }
}

#[async_std::main]
async fn main() {
    loop {
        // do master connection loop
        if let Ok(_stream) = TcpStream::connect(CONFIG.master).await {
            let stream: Arc<Mutex<TcpStream>> = Arc::new(Mutex::new(_stream));
            let key = EphemeralSecret::random(thread_rng());
            let state = Arc::new(Mutex::new(State {
                key: Arc::new(key),
                shared: Arc::new(Mutex::new(None)),
            }));
            // Send Handshake packet
            let Handshake = Packet::make_packet(
                Command::Handshake,
                bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .serialize(&state.lock().await.key.public_key())
                    .unwrap(),
            );
            Handshake.send(Pin::new(stream.lock().await.by_ref())).await.ok();
            loop {
                // TODO: check connection
                // packet loop
                if let Ok(packet) =
                    Packet::from_stream(Pin::new(stream.lock().await.by_ref())).await
                {
                    let state_mutex = Arc::clone(&state);
                    let stream_mutex = Arc::clone(&stream);
                    spawn(async move {
                        state_mutex
                            .lock()
                            .await
                            .handle_command(stream_mutex.lock().await.by_ref(), packet)
                            .await
                    });
                } else {
                    debug!("Wrong packet as received");
                }
            }
        }
    }
}
