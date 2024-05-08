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

pub async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    peer_map.lock().unwrap().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let messages_incoming = incoming.try_for_each(|msg| {
        println!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );
        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(messages_incoming, receive_from_others);
    future::select(messages_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.clone().lock().unwrap().remove(&addr);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = 4;
        assert_eq!(result, 4);
    }
}
