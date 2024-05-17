use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::protocol::Message;
pub mod error;

pub type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;
type Tx = UnboundedSender<Message>;

// Our helper method which will read data from stdin and send it along the
// sender provided.
pub async fn read_stdin() -> Vec<u8> {
    let mut stdin = tokio::io::stdin();
    let mut buf = vec![0; 1024];
    let n = match stdin.read(&mut buf).await {
        Err(_) | Ok(0) => panic!(),
        Ok(n) => n,
    };
    buf.truncate(n);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClient {
        // Receiver from websocket connection
        read: mpsc::Receiver<ServerResponse>,
        // Sender to websocket connection
        write: mpsc::Sender<ServerMessage>,
    }

    #[test]
    fn it_works() {
        let result = 4;
        assert_eq!(result, 4);
    }
}
