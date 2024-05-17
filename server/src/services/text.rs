use tokio::sync::mpsc::{channel, Receiver, Sender};
use tungstenite::Message;

#[derive(Debug, thiserror::Error)]
pub enum TextStreamerError {
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
}

pub struct TextStreamer {
    clients: Vec<Sender<Message>>,
    pub sender: Sender<Message>,
    receiver: Receiver<Message>,
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

    pub async fn suscribe(mut self, sender: Sender<Message>) {
        self.clients.push(sender);
    }

    pub async fn send(&mut self, message: Message) {
        self.clients.iter().map(|client| {
            client.send(Message::Text("Hello client!".to_string()));
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
