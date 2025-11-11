use std::env;
use std::io;
use std::path::PathBuf;
mod client_factory;
mod client_handler;
mod mqtt;
mod sequencer;
mod services;
//mod test; Tests are all broken, will fix them as soon as possible.

use crate::client_factory::ClientBuilder;
use crate::sequencer::Sequencer;
use crate::services::service::ColourService;
use crate::services::service::MidiService;
use crate::services::service::PlaybackService;
use crate::services::service::SubtitleService;
use crate::services::LamarrsService;
use clap::Parser;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use mqtt::MqttInterface;
use tokio::net::TcpListener;
use tracing::instrument;
use tracing::{debug, error, info};
use tracing_subscriber::filter::{EnvFilter, ParseError};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

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

// Configure a subscriber that output logs to stdout.
fn configure_logging(level: &str) -> Result<(), ParseError> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_new(level)?)
        .init();
    Ok(())
}

#[derive(Parser, Debug)]
pub struct Args {
    /// The verbosity of the application, options are TRACE, DEBUG, INFO, WARN and ERROR.
    #[arg(long, default_value = "INFO")]
    pub log_level: String,
    /// Server hostname or IP
    #[arg(short, long)]
    pub server_ip: String,
    /// Port number
    #[arg(short, long, default_value_t = 8080)]
    pub port: u16,
    /// Relative Path to the executable where to look for the sequencer yaml
    /// file with the show list of instructions.
    #[arg(long)]
    pub sequence_path: PathBuf,
}

#[tokio::main]
#[instrument(err)]
async fn main() -> Result<()> {
    // `main` uses color eyre to show the custom errors in a human readable way if anything
    // crashes the program.
    color_eyre::install()?;
    info! {VERSION};

    // Parsing of CLI arguments.
    let args = Args::parse();
    configure_logging(&args.log_level).expect("Failed to configure logging to stdout.");

    // Creates the event loop and TCP listener we'll accept connections on.
    let server_ip_addr = format!("{}:{}", args.server_ip, args.port);
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
    debug!("Creating MidiService");
    let mut midi_service= MidiService::new();
    debug!("Creating MQTT Interface");
    let mut mqtt_interface = MqttInterface::new(
        subtitle_service.sender.clone(),
        colour_service.sender.clone(),
        playback_service.sender.clone(),
        midi_service.sender.clone(),
    );
    debug!("Creating Sequencer Service");
    let mut sequencer = Sequencer::new(
        subtitle_service.sender.clone(),
        colour_service.sender.clone(),
        playback_service.sender.clone(),
        midi_service.sender.clone(),
        args.sequence_path,
    );

    debug!("Creating Client builder");
    let client_builder = ClientBuilder::new(
        subtitle_service.sender.clone(),
        colour_service.sender.clone(),
        playback_service.sender.clone(),
        midi_service.sender.clone(),
        sequencer.sender.clone(),
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
        result = midi_service.run() => {
            Err(eyre!("MQTT service crashed: {:?}", result))?
        }
        result = mqtt_interface.run() => {
            Err(eyre!("MQTT service crashed: {:?}", result))?
        }
        result = sequencer.run() => {
            Err(eyre!("Sequencer crashed: {:?}", result))?
        }
        result = client_builder.run(listener) => {
            Err(eyre!("Client builder service crashed: {:?}", result))?
        }
    }
    Ok(())
}
