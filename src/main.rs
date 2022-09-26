#[macro_use]
extern crate lazy_static;

mod config;
mod container;
mod language;

use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::task::{block_on, spawn};
use judge_protocol::constants::*;
use judge_protocol::packet::*;
use k256::ecdh::EphemeralSecret;
use k256::ecdh::SharedSecret;
use k256::PublicKey;
use rand::thread_rng;
use std::fs::read_to_string;

use async_std::sync::*;

use config::Config;
use language::Languages;

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
            Command::HANDSHAKE => {
                if let Ok(server_pubkey) = bincode::deserialize::<PublicKey>(&packet.heady.body) {
                    self.shared =
                        Arc::new(Mutex::new(Some(self.key.diffie_hellman(&server_pubkey))));
                } else {
                    debug!("[PMS-slave] An error occured");
                }
            }
            _ | Command::UNKNOWN => {
                debug!("[PMS-slave] An unknown command has received");
                // Unknown
            }
        }
    }
}

fn main() -> Result<()> {
    block_on(async {
        loop {
            // do master connection loop
            if let Ok(_stream) = TcpStream::connect(CONFIG.master).await {
                let stream: Arc<Mutex<TcpStream>> = Arc::new(Mutex::new(_stream));
                let key = EphemeralSecret::random(thread_rng());
                let state = Arc::new(Mutex::new(State {
                    key: Arc::new(key),
                    shared: Arc::new(Mutex::new(None)),
                }));
                // Send HANDSHAKE packet
                let handshake = Packet::make_packet(
                    Command::HANDSHAKE,
                    bincode::serialize(&state.lock().await.key.public_key()).unwrap(),
                );
                stream
                    .lock()
                    .await
                    .write_all(&bincode::serialize(&handshake).unwrap())
                    .await?;
                loop {
                    let mut buf: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
                    stream.lock().await.read_exact(&mut buf).await?;
                    if let Ok(header) = bincode::deserialize::<PacketHeader>(&buf) {
                        if header.check_magic() {
                            let mut buf_end: Vec<u8> = Vec::new();
                            buf_end.resize((header.length as usize) + 16, 0);
                            stream
                                .lock()
                                .await
                                .read_exact(buf_end.as_mut_slice())
                                .await?;
                            let mut buf_all: Vec<u8> = buf.to_vec();
                            buf_all.append(&mut buf_end);
                            if let Ok(packet) = bincode::deserialize::<Packet>(&buf_all) {
                                if packet.verify() {
                                    let state_mutex = Arc::clone(&state);
                                    let stream_mutex = Arc::clone(&stream);
                                    spawn(async move {
                                        state_mutex
                                            .lock()
                                            .await
                                            .handle_command(
                                                stream_mutex.lock().await.by_ref(),
                                                packet,
                                            )
                                            .await
                                    });
                                } else {
                                    debug!("[PMS-slave] Wrong checksum");
                                }
                            } else {
                                debug!("[PMS-slave] Wrong packet type");
                            }
                        } else {
                            debug!("[PMS-slave] Wrong magic was received");
                        }
                    }
                }
            }
        }
    })
}
