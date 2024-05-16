use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::{
    io,
    sync::mpsc::{channel, Receiver, Sender},
};
use tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum ConnManagerError {
    #[error("An error ocurred while trying to bind the TCP Listener")]
    Error(#[from] io::Error),
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
}

pub struct WebSocketFactory {
    text: Sender<Message>,
    color: Sender<Message>,
    pub sender: Sender<Message>,
    receiver: Receiver<Message>,
}

impl WebSocketFactory {
    pub fn new(text: Sender<Message>, color: Sender<Message>) -> Self {
        let (sender, receiver) = channel(32);
        Self {
            text,
            color,
            sender,
            receiver,
        }
    }

    pub async fn run(self, listener: TcpListener) -> Result<(), ConnManagerError> {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tokio::spawn(spawn_client_connection(
                        stream,
                        self.text.clone(),
                        self.color.clone(),
                        addr,
                    ));
                }
                Err(error) => return Err(ConnManagerError::Error(error)),
            }
        }
    }
}

async fn spawn_client_connection(
    stream: TcpStream,
    text: Sender<Message>,
    color: Sender<Message>,
    addr: SocketAddr,
) -> Result<(), ConnManagerError> {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, mut rx) = channel(32);

    let (mut outgoing, mut incoming) = ws_stream.split();
    loop {
        tokio::select!(
            message = incoming.next() => {
                if let Some(message) = message {
                    let result = message.unwrap();
                    println!(
                        "Received a message from {}: {}",
                        addr,
                        result.to_text().unwrap()
                    );
                    // This is a horrible patch to allow clients to disconnect, it will be solved after implementing the Messages enum.
                    if result.to_text().unwrap() == "disconnect\n" {
                        println!("Client wants to disconnect");
                        break
                    }
                }
            }
            message = rx.recv() => {
                if let Some(message) = message {
                    let _ = outgoing.send(message).await;
                }
            }
        )
    };
    println!("{} disconnected", &addr);
    Ok(())
}
