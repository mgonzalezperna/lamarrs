use std::{collections::HashMap, io, sync::Mutex};

use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("An error ocurred while trying to bind the TCP Listener")]
    Error(#[from] io::Error),
    #[error("The message has to be a valid UTF8 string.")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}

pub async fn spawn(addr: String) -> Result<(), ServerError> {
    let state = lamarrs_utils::PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket?;
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    tokio::spawn(write_to_peers(state.clone()));
    loop{
        match listener.accept().await {
            Ok((stream, addr)) => {
                tokio::spawn(lamarrs_utils::handle_connection(state.clone(), stream, addr));       
            },
            Err(error) => return Err(ServerError::Error(error))
        }
    }
}

async fn write_to_peers(peer_map: lamarrs_utils::PeerMap) -> Result<(), ServerError> {
    loop {
        let stdin = lamarrs_utils::read_stdin().await;
        let message = Message::Text(String::from_utf8(stdin.clone())?);
        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers.iter().map(|(_, ws_sink)| ws_sink);
        for recp in broadcast_recipients {
            recp.unbounded_send(message.clone()).unwrap();
        }
    }
}
