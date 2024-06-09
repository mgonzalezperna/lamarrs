use futures_util::{SinkExt, StreamExt};
use lamarrs_utils::enums::{self, GatewayMessage, RelativeLocation, SubscriberMessage};
use tokio::{
    io,
    sync::mpsc::{self, error::SendError},
};
use tungstenite::Message;
use url::Url;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum FakeSubscriberError {
    #[error("Connection could't not be stablished with the server.")]
    Error(#[from] tungstenite::error::Error),
    #[error("The message has to be a valid UTF8 string.")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}

#[derive(Debug)]
pub struct FakeSubscriber {
    // WS server URL
    location: RelativeLocation,
    id: Uuid,
    url: Url,
    // Receiver from websocket connection
    read: mpsc::Receiver<GatewayMessage>,
    // Sender to websocket connection
    write: mpsc::Sender<SubscriberMessage>,
}

impl FakeSubscriber {
    pub async fn new(url: Url, location: RelativeLocation) -> Self {
        let id = Uuid::new_v4();
        let (tx_send, tx_recv) = mpsc::channel(1);
        let (rx_send, rx_recv) = mpsc::channel(1);

        Self {
            id,
            location,
            url,
            read: rx_recv,
            write: tx_send,
        }
    }

    pub async fn recv(&mut self) -> Option<GatewayMessage> {
        self.read.recv().await
    }

    /// Send an message to the server
    pub async fn send(
        &mut self,
        message: SubscriberMessage,
    ) -> Result<(), SendError<SubscriberMessage>> {
        self.write.send(message).await
    }

    /// Close any open websocket connections and stop `FakeClient`.
    pub async fn stop(&mut self) {
        self.read.close();
        self.write
            .send(SubscriberMessage::CloseConnection(
                enums::CloseConnectionReason::SubscriberRequest,
            ))
            .await
            .unwrap();
    }

    pub async fn start(&mut self) {
        let (tx_send, tx_recv) = mpsc::channel(1);
        let (rx_send, rx_recv) = mpsc::channel(1);

        self.write = tx_send;
        self.read = rx_recv;

        tokio::spawn(listen(self.url.clone(), tx_recv, rx_send));
    }

    pub async fn register(&mut self) {
        self.send(SubscriberMessage::Register((
            self.id,
            self.location.clone(),
        )))
        .await;
    }

    pub async fn restart(&mut self) {
        self.stop().await;
        self.start().await;
    }
}

async fn listen(
    url: Url,
    mut read: mpsc::Receiver<SubscriberMessage>,
    write: mpsc::Sender<GatewayMessage>,
) -> Result<(), FakeSubscriberError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;

    let (mut sink, mut stream) = ws_stream.split();

    // Listen to messages from websocket connection and respond to a channel
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Text(msg) = msg {
                let unwrapped_message = serde_json::from_str(&msg).unwrap();
                if write.send(unwrapped_message).await.is_err() {
                    break;
                }
            };
        }
    });

    // Listen to messages from a channel and write to websocket connection
    tokio::spawn(async move {
        while let Some(msg) = read.recv().await {
            if let SubscriberMessage::CloseConnection(_) = msg {
                sink.send(tungstenite::Message::Close(None)).await;
                sink.close().await.unwrap();
                break;
            };
            sink.send(tungstenite::Message::Text(
                serde_json::to_string(&msg).unwrap(),
            ))
            .await;
        }
    });
    Ok(())
}
