use async_std::channel::Sender;
use async_std::net::TcpStream;
use async_std::sync::Arc;
use async_std::task::sleep;

use std::time::Duration;

use crate::constants::CHECK_ALIVE_TIME;
use crate::protocol::Actions;

pub async fn check_alive(action_tx: Sender<Actions>, stream: Arc<TcpStream>) {
    loop {
        sleep(Duration::from_secs(CHECK_ALIVE_TIME)).await;
        if let Ok(val) = stream.peek(&mut vec![0]).await {
            if val == 0 {
                action_tx.send(Actions::Reconnect(0)).await.ok();
            }
        }
    }
}
