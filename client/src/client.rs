use futures_util::{future, pin_mut, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("No websocket server found in the provided URL.")]
    ParseError(#[from] url::ParseError),
    #[error("Connection could't not be stablished with the server.")]
    Error(#[from] tungstenite::error::Error),
    
}

pub async fn spawn(connect_addr: String) -> Result<(), ClientError> {
    let url = url::Url::parse(&connect_addr)?;

    let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
    tokio::spawn(async move {
        loop {
            let stdin = lamarrs_utils::read_stdin().await;
            stdin_tx.unbounded_send(Message::binary(stdin)).unwrap();
        }
    });
    let (ws_stream, _) = connect_async(url).await?;
    println!("WebSocket handshake has been successfully completed");

    let (write, read) = ws_stream.split();

    let stdin_to_ws = stdin_rx.map(Ok).forward(write);
    let ws_to_stdout = {
        read.for_each(|message| async {
            let data = message.unwrap().into_data();
            tokio::io::stdout().write_all(&data).await.unwrap();
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
    Ok(())
}