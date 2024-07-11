use std::env;
use std::io;
mod mqtt;
mod services;
mod subscriber;
mod test;
mod ws_factory;

use crate::services::text_streamers::SubtitlesStreamer;
use crate::ws_factory::SubscriberBuilder;
use mqtt::MqttInterface;
use services::sound_streamers::MidiStreamer;
use services::text_streamers::ColorStreamer;
use tokio::net::TcpListener;
use tracing::{debug, error, info};
use tracing_subscriber::filter::{EnvFilter, ParseError};

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("An error ocurred while trying to bind the TCP Listener")]
    Error(#[from] io::Error),
    #[error("The message has to be a valid UTF8 string.")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("There was a irrecoverable error initialising the SubtitlesService.")]
    SubtitleServiceError,
    #[error("There was a irrecoverable error initialising the ColorService.")]
    ColorServiceError,
    #[error("There was a irrecoverable error initialising the MidiService.")]
    MidiServiceError,
    #[error("There was a irrecoverable error initialising the WebSocketService.")]
    WebSocketFactoryError,
}

fn configure_logging(level: &str) -> Result<(), ParseError> {
    let env_filter = EnvFilter::try_new(level)?.add_directive(
        "rumqttc=info"
            .parse()
            .expect("Failed to set log level of rumqttc to info"),
    );

    let format = tracing_subscriber::fmt::format()
        .with_source_location(true)
        .with_target(false);

    tracing_subscriber::fmt()
        .event_format(format)
        .with_env_filter(env_filter)
        .init();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    configure_logging("debug");
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket?;
    println!("Listening on: {}", addr);
    let mut subtitle_service = SubtitlesStreamer::new();
    let mut color_service = ColorStreamer::new();
    let mut midi_service = MidiStreamer::new();
    let mut mqtt_interface = MqttInterface::new(
        subtitle_service.sender.clone(),
        color_service.sender.clone(),
    );
    let ws_factory = SubscriberBuilder::new(
        subtitle_service.sender.clone(),
        color_service.sender.clone(),
        midi_service.sender.clone(),
    );

    tokio::select! {
        _ = subtitle_service.run() => {
            Err(ServerError::SubtitleServiceError)
        }
        _ = color_service.run() => {
            Err(ServerError::ColorServiceError)
        }
        _ = midi_service.run() => {
            Err(ServerError::MidiServiceError)
        }
        _ = mqtt_interface.run() => {
            Err(ServerError::WebSocketFactoryError)
        }
        _ = ws_factory.run(listener) => {
            Err(ServerError::WebSocketFactoryError)
        }
    }
}
