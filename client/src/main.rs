mod services;
mod server_handler;

use clap::{Parser, ValueEnum};
use color_eyre::eyre::{Result, WrapErr, eyre};
use std::path::PathBuf;
//use lamarrs_utils::RelativeLocation;
use http::Uri;
use tracing::{debug, info};
use tracing_subscriber::{
    filter::ParseError, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

use lamarrs_utils::{AudioFile, ColourRgb, Service as ServerService};
use std::path::Path;
use tokio::sync::mpsc::Sender;
use crate::{server_handler::{Client, ServerHandlerError}, services::playback_service::{PlaybackService, PlaybackServiceError}};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// This is a distorted re-export of the ActionMessage from utils.
/// Moving this to the lamarrs-utils doesn't make sense right now because
///  a- It is not needed in any other workspace
///  b- Would force lamarrs-utils to have Tokio as dependency, which would break web-ui as Tokio doesn't play nice with WASM and no-std.
/// TODO: Add these to `utils` and gate them under a feature flag: https://stackoverflow.com/questions/75599346/what-are-the-consequences-of-a-feature-gated-enum-variant
/// Internal message types to be transmited between actors inside Lamarrs client.
/// These are also the payloads the clients will be sending inside the Exchange Messages.
#[derive(Debug)]
pub enum InternalEventMessageClient {
    ConnectedToServer(Sender<InternalEventMessageClient>),
    SubscribeToService(ServerService),
    PlayAudio(AudioFile, Sender<InternalEventMessageClient>),
    ShowSubtitles(String, Sender<InternalEventMessageClient>),
    NewDmxColour(ColourRgb, Sender<InternalEventMessageClient>),
    NewLedColour(ColourRgb, Sender<InternalEventMessageClient>),
    Config(serde_json::Value), // Joker kind, must dissapear in the future.
}

#[derive(Debug, thiserror::Error)]
pub enum LamarrsClientError {
    #[error("Playback service failed: {0}")]
    PlaybackService(#[from] PlaybackServiceError), 
    #[error("Server handler failed: {0}")]
    ServerHandler(#[from] ServerHandlerError), 
    #[error("There was an error configuring the Lamarrs client: {}", err_desc)]
    ClientConfig { err_desc: String },
}

/// Re-exported mirrored enum from lamarrs-utils::Service
/// in order to implement ValueEnum.
/// This should be changed to all the supported cases the clients will have.
/// i.e. DMX, LED, Screen subtitles, 7 segment boards, MIDI controllers, playback...
/// This will do by now.
#[derive(ValueEnum, PartialEq, Clone, Debug)]
pub enum Service {
    Subtitle,
    Colour,
    Playback,
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
    pub server: String,
    /// Port number
    #[arg(short, long, default_value_t = 8080)]
    pub port: u16,
    /// Relative position of the client. Unsupported by now.
    //#[arg(long, default_value=None)]
    //pub location: Option<RelativeLocation>,
    /// Relative Path to the executable where to look for the media to be used by
    /// the Client Services that executes files.
    #[arg(long)]
    pub media_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // `main` uses color eyre to show the custom errors in a human readable way if anything
    // crashes the program.
    color_eyre::install()?;
    info! {VERSION};
    // Parsing of CLI arguments.
    let args = Args::parse();
    configure_logging(&args.log_level).expect("Failed to configure logging to stdout.");
    // Service creation.
    // Order of creation is important, since the channels
    // pipelines to send and receive messages to each Actor.
    debug!("Creating Playback Service");
    let mut playback_service = PlaybackService::new(args.media_path);

    let server_address = Uri::builder()
        .scheme("ws")
        .authority(format!("{}:{}", args.server, args.port))
        .path_and_query("/")
        .build()?;

    debug!("Creating Client actor");
    let mut client_builder = Client::new(
        None, // We are not supporting location for this iteration of the Client.
        server_address,
        playback_service.sender.clone(),
        // subtitle_service.sender.clone(),
        // colour_service.sender.clone(),
    );
    tokio::select! {
        result = playback_service.run() => {
            Err(eyre!("Playback service crashed: {:?}", result))?
        }
        result = client_builder.run() => {
            Err(eyre!("Server handler crashed: {:?}", result))?
        }
    }
    Ok(())
}