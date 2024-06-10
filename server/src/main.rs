use std::env;
use std::io;
mod services;
mod subscriber;
mod test;
mod ws_factory;

use crate::services::text_streamers::SubtitlesStreamer;
use crate::ws_factory::SubscriberBuilder;
use services::text_streamers::ColorStreamer;
use tokio::net::TcpListener;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("An error ocurred while trying to bind the TCP Listener")]
    Error(#[from] io::Error),
    #[error("The message has to be a valid UTF8 string.")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("There was a irrecoverable error initialising the TextService.")]
    TextServiceError,
    #[error("There was a irrecoverable error initialising the WebSocketService.")]
    WebSocketFactoryError,
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket?;
    println!("Listening on: {}", addr);
    let mut subtitle_service = SubtitlesStreamer::new();
    let mut color_service = ColorStreamer::new();
    let ws_factory = SubscriberBuilder::new(
        subtitle_service.sender.clone(),
        color_service.sender.clone(),
    );
    tokio::select! {
        _ = subtitle_service.run() => {
            Err(ServerError::TextServiceError)
        }
        _ = ws_factory.run(listener) => {
            Err(ServerError::WebSocketFactoryError)
        }
    }
}
