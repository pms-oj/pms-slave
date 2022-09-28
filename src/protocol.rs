use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::task::spawn;

use bincode::Options;

use judge_protocol::handshake::*;
use judge_protocol::packet::*;
use k256::ecdh::EphemeralSecret;
use k256::ecdh::SharedSecret;
use k256::PublicKey;
use rand::thread_rng;
use async_std::task::sleep;

use async_std::channel::{unbounded, Receiver, Sender};
use async_std::sync::*;

use std::pin::Pin;
use std::time::Duration;

use crate::{CONFIG, LANGUAGES};
use log::{debug, info, error};

#[derive(Clone, Copy, Debug)]
enum Actions {
    Reconnect,
    Unknown,
}

struct State {
    key: Arc<EphemeralSecret>,
    node_id: Mutex<u32>,
    shared: Arc<Mutex<Option<SharedSecret>>>,
    signal: Arc<Mutex<Sender<Actions>>>,
}

impl State {
    async fn verify_token(&mut self, mut stream: &mut TcpStream) -> async_std::io::Result<()> {
        let body = BodyAfterHandshake::<PublicKey> {
            node_id: (*self.node_id.lock().await),
            req: self.key.public_key(),
        };
        let packet = Packet::make_packet(Command::VerifyToken, body.bytes());
        packet.send(Pin::new(&mut stream)).await
    }

    async fn handle_command(&mut self, stream: &mut TcpStream, packet: Packet) {
        match packet.heady.header.command {
            Command::Handshake => {
                if let Ok(res) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<HandshakeResult>(&packet.heady.body)
                {
                    self.node_id = Mutex::new(res.node_id);
                    self.shared = Arc::new(Mutex::new(Some(
                        self.key.diffie_hellman(&res.server_pubkey),
                    )));
                } else {
                    debug!("An error occurred");
                }
            }
            Command::ReqVerifyToken => {
                if let Ok(state) = bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .deserialize::<bool>(&packet.heady.body)
                {
                    if !state {
                        debug!("Session was expired. Trying to reconnect ...");
                        self.signal.lock().await.send(Actions::Reconnect).await;
                    }
                } else {
                    debug!("An error occurred");
                }
            }
            _ => {
                debug!("An unknown command has received");
                // Unknown
            }
        }
    }
}

pub async fn open_protocol() {
    loop {
        // do master connection loop
        if let Ok(_stream) = TcpStream::connect(CONFIG.master).await {
            let stream: Arc<Mutex<TcpStream>> = Arc::new(Mutex::new(_stream));
            let key = EphemeralSecret::random(thread_rng());
            let (send, recv): (Sender<Actions>, Receiver<Actions>) = unbounded();
            let state = Arc::new(Mutex::new(State {
                key: Arc::new(key),
                node_id: Mutex::new(std::u32::MAX),
                shared: Arc::new(Mutex::new(None)),
                signal: Arc::new(Mutex::new(send)),
            }));
            // Send Handshake packet
            let handshake = Packet::make_packet(
                Command::Handshake,
                bincode::DefaultOptions::new()
                    .with_big_endian()
                    .with_fixint_encoding()
                    .serialize(&state.lock().await.key.public_key())
                    .unwrap(),
            );
            handshake
                .send(Pin::new(stream.lock().await.by_ref()))
                .await
                .ok();
            loop {
                if let Ok(actions) = recv.try_recv() {
                    match actions {
                        Actions::Reconnect => {
                            break;
                        }
                        _ => {}
                    }
                }
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
            drop(state);
            drop(recv);
        } else {
            error!("Cannot connect to server. Trying to connect in 5 secs ...");
            sleep(Duration::from_secs(5)).await;
        }
    }
}
