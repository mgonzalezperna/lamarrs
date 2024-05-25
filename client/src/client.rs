use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio::sync::mpsc::{channel, Sender};
use lamarrs_utils::enums::SubscriberMessage;
use lamarrs_utils::messages::Subscribe;
use arrayvec::ArrayString;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("No websocket server found in the provided URL.")]
    ParseError(#[from] url::ParseError),
    #[error("Connection could't not be stablished with the server.")]
    Error(#[from] tungstenite::error::Error),
}

pub async fn spawn(connect_addr: String) -> Result<(), ClientError> {
    let url = url::Url::parse(&connect_addr)?;

    let (mut stdin_tx, mut stdin_rx) = channel(32);
    tokio::spawn(async move {
        loop {
            let stdin = lamarrs_utils::read_stdin().await;
            let msg = SubscriberMessage::Subscribe(Subscribe{service: ArrayString::from(std::str::from_utf8(&stdin).unwrap()).unwrap()});
            dbg!(&msg);
            stdin_tx.send(msg).await;
            //stdin_tx.unbounded_send(Message::binary(stdin)).unwrap();
        }
    });
    let (ws_stream, _) = connect_async(url).await?;
    println!("WebSocket handshake has been successfully completed");

    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select!(
            msg = read.next() => {
                if let Some(Ok(msg)) = msg {
                    let data = msg.into_data();
                    tokio::io::stdout().write_all(&data).await.unwrap();
            }}
            msg = stdin_rx.recv() => {
                if let Some(msg) = msg {
                    let item = tungstenite::Message::Text(serde_json::to_string(&msg).unwrap());
                    dbg!(&item);
                    let _ = write.send(item).await;
                }
            }
        );
    }
    Ok(())
}
