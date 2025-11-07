use std::env;
use std::io;
mod client_factory;
mod client_handler;
mod mqtt;
mod services;
//mod test; Tests are all broken, will fix them as soon as possible.

use crate::client_factory::ClientBuilder;
use crate::services::service::ColourService;
use crate::services::service::PlaybackService;
use crate::services::service::SubtitleService;
use crate::services::LamarrsService;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use mqtt::MqttInterface;
use tokio::net::TcpListener;
use tracing::instrument;
use tracing::{debug, error, info};
use tracing_subscriber::filter::{EnvFilter, ParseError};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, thiserror::Error)]
pub enum LamarrsServerError {
    #[error("There was an error configuring the Lamarrs server: {}", err_desc)]
    ServerConfig { err_desc: String },
    #[error("An error ocurred while trying to bind the TCP Listener")]
    TcpListener(#[from] io::Error),
    #[error("The message has to be a valid UTF8 string.")]
    FromUtf8(#[from] std::string::FromUtf8Error),
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
#[instrument(err)]
async fn main() -> Result<()> {
    // `main` uses color eyre to show the custom errors in a human readable way if anything
    // crashes the program.
    color_eyre::install()?;
    info! {VERSION};

    let server_ip_addr = env::args().nth(1).ok_or(LamarrsServerError::ServerConfig {
        err_desc: "The address:port to serve Lamarrs was not provided".into(),
    })?;
    configure_logging("debug").map_err(|e| LamarrsServerError::ServerConfig {
        err_desc: e.to_string(),
    })?;

    // Creates the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&server_ip_addr).await;
    let listener = try_socket?;
    info!("Listening on: {}", server_ip_addr);

    // Service creation.
    // Order of creation is important, since the channels
    // pipelines to send and receive messages to each Actor.
    debug!("Creating SubtitleService");
    let mut subtitle_service = SubtitleService::new();
    debug!("Creating ColourService");
    let mut colour_service = ColourService::new();
    debug!("Creating PlaybackService");
    let mut playback_service = PlaybackService::new();
    debug!("Creating MQTT Interface");
    let mut mqtt_interface = MqttInterface::new(
        subtitle_service.sender.clone(),
        colour_service.sender.clone(),
        playback_service.sender.clone(),
    );
    debug!("Creating Client builder");
    let client_builder = ClientBuilder::new(
        subtitle_service.sender.clone(),
        colour_service.sender.clone(),
        playback_service.sender.clone(),
    );

    tokio::select! {
        result = subtitle_service.run() => {
            Err(eyre!("Subtitles service crashed: {:?}", result))?
        }
        result = colour_service.run() => {
            Err(eyre!("Colour service crashed: {:?}", result))?
        }
        result = playback_service.run() => {
            Err(eyre!("Playback service crashed: {:?}", result))?
        }
        result = mqtt_interface.run() => {
            Err(eyre!("MQTT service crashed: {:?}", result))?
        }
        result = client_builder.run(listener) => {
            Err(eyre!("Client builder service crashed: {:?}", result))?
        }
    }
    Ok(())
}
