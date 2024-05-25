use futures_util::{SinkExt, StreamExt};
use lamarrs_utils::enums::GatewayMessage;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::{
    io,
    sync::mpsc::{channel, Sender},
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
    text: Sender<GatewayMessage>,
    color: Sender<GatewayMessage>,
}

impl WebSocketFactory {
    pub fn new(text: Sender<GatewayMessage>, color: Sender<GatewayMessage>) -> Self {
        Self {
            text,
            color,
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
    text: Sender<GatewayMessage>,
    color: Sender<GatewayMessage>,
    addr: SocketAddr,
) -> Result<(), ConnManagerError> {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    println!("WebSocket connection established: {}", addr);

    let (tx, mut rx) = channel(32);

    let (mut outgoing, mut incoming) = ws_stream.split();
    loop {
        tokio::select!(
            message = incoming.next() => {
                //dbg!(&message);
                match message {
                    Some(Ok(message)) => {
                        println!(
                            "Received a message from {}: {}",
                            addr,
                            &message.to_text().unwrap() 
                        );
                        let subscriber_message = match message {
                            Message::Text(data) => serde_json::to_value(data).unwrap(),
                            Message::Close(_) => {
                                println!("Client wants to disconnect");
                                break
                            },
                            _ => panic!("Unrecognized message")
                        };
                    },
                    Some(Err(error)) => panic!("Error! {}", error),
                    None => {
                        outgoing.close().await;
                        break;
                    },
                    _ => println!("Unknown incoming message")
                }
            }
            message = rx.recv() => {
                if let Some(message) = message {
                    let _ = outgoing.send(message).await;
                }
            }
        )
    }
    println!("{} disconnected", &addr);
    Ok(())
}

#[cfg(test)]
pub mod test {
    use crate::test::FakeClient;

   // pub async fn create_ws_for_new_client() {
   //     let mut fake_client: FakeClient = FakeClient::new("localhost".to_string());
   //     let result = 4;
   //     assert_eq!(result, 4);
   // }
}
