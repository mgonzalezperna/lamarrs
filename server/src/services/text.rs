use tokio::sync::mpsc::{channel, Receiver, Sender};
use lamarrs_utils::{messages::{Location, Subtitle, Subscribe}, enums::GatewayMessage};
use arrayvec::ArrayString;

#[derive(Debug, thiserror::Error)]
pub enum TextStreamerError {
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
}

/// Messages [`TextStreamer`] operates on.
#[derive(Debug)]
pub enum Message {
    // A Client wants to subscribe to the service.
    Subscribe,

    // A Client wants to update its location.
    Location,
}


pub struct TextStreamer {
    clients: Vec<Sender<GatewayMessage>>,
    pub sender: Sender<GatewayMessage>,
    receiver: Receiver<GatewayMessage>,
}

impl TextStreamer {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            clients: vec![],
            sender,
            receiver,
        }
    }

    pub async fn suscribe(mut self, sender: Sender<GatewayMessage>) {
        self.clients.push(sender);
    }

    pub async fn send(&mut self, message: GatewayMessage) {
        self.clients.iter().map(|client| {
            client.send(GatewayMessage::Subtitle(Subtitle{subtitle: ArrayString::from("Hello client!").unwrap()}));
        });
    }

    pub async fn run(&mut self) -> Result<(), TextStreamerError> {
        loop {
            while let Some(message) = self.receiver.recv().await {
                let _ = self.send(message).await;
            }
        }
    }
}
