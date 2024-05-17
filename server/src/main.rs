use std::env;
use std::io;
mod services;
mod ws_factory;

use crate::services::Text;
use crate::ws_factory::WebSocketFactory;
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
    let mut text_service = Text::new();
    let ws_factory =
        WebSocketFactory::new(text_service.sender.clone(), text_service.sender.clone());
    tokio::select! {
        _ = text_service.run() => {
            Err(ServerError::TextServiceError)
        }
        _ = ws_factory.run(listener) => {
            Err(ServerError::WebSocketFactoryError)
        }
    }
}
