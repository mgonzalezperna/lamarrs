use std::{
    fmt::{self},
    io,
    path::{Path, PathBuf},
};

use lamarrs_utils::{AudioFile, Service};
use rodio::StreamError;
use tokio::sync::mpsc::{self, channel, Receiver, Sender};
use tracing::info;

use crate::InternalEventMessageClient;

#[derive(Debug, thiserror::Error)]
pub enum PlaybackServiceError {
    #[error("There was a irrecoverable error with the service {}.", service)]
    Service { service: String },
    #[error("Error sending an InternalMessage to Server handler")]
    SendInternalMessage(#[from] mpsc::error::SendError<InternalEventMessageClient>),
    #[error("Failed opening an audio output stream")]
    FailedOpeningOutputStream(#[from] StreamError),
    #[error("Target audio file failed to open. Path is wrong or file is corrupted.")]
    FailedOpeningTargetAudioFile(#[from] io::Error),
    #[error("Target audio file can't be decoded. Format may not be supported.")]
    FailedPlayingTargetAudioFile(#[from] rodio::decoder::DecoderError),
}

#[derive(Debug)]
pub struct PlaybackService {
    pub sender: Sender<InternalEventMessageClient>,
    receiver: Receiver<InternalEventMessageClient>,
    media_path: PathBuf,
}

impl PlaybackService {
    pub fn new(media_path: PathBuf) -> Self {
        let (sender, receiver) = channel(32);
        Self { sender, receiver, media_path }
    }
}

// Implement `Display` for `PlaybackService`.
impl fmt::Display for PlaybackService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PlaybackService")
    }
}

impl PlaybackService {
    async fn receive_message(&mut self) -> Option<InternalEventMessageClient> {
        self.receiver.recv().await
    }

    /// Runs the Service.
    pub async fn run(&mut self) -> Result<(), PlaybackServiceError> {
        loop {
            while let Some(message) = self.receive_message().await {
                match message {
                    InternalEventMessageClient::ConnectedToServer(sender) => {
                        self.subscribe_to_remote_service(sender).await?
                    }
                    InternalEventMessageClient::PlayAudio(audio_file_path, sender) => {
                        self.play_audio(audio_file_path, sender).await?
                    }
                    InternalEventMessageClient::Config(_) => {
                        unimplemented!("This message is not yet functional.")
                    }
                    _ => {
                        return Err(PlaybackServiceError::Service {
                            service: self.to_string(),
                        })
                    }
                }
            }
        }
    }

    /// Each service must know what remote services needs to consume. A Client Service could
    /// several different messages if required.
    async fn subscribe_to_remote_service(
        &mut self,
        server_sender: Sender<InternalEventMessageClient>,
    ) -> Result<(), PlaybackServiceError> {
        info!("{} subscribing to Server AudioPlayer", self.to_string());
        Ok(server_sender
            .send(InternalEventMessageClient::SubscribeToService(
                Service::AudioPlayer,
            ))
            .await?)
    }

    async fn play_audio(
        &mut self,
        audio_file_data: AudioFile,
        sender: Sender<InternalEventMessageClient>,
    ) -> Result<(), PlaybackServiceError> {
        info!("Playing audio file {:?}.", audio_file_data);
        let audio_file_path = Path::join(&self.media_path, audio_file_data.file_name_with_extension().to_string());
        let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
        let sink = rodio::Sink::connect_new(stream_handle.mixer());

        info!("Playing {:?}", audio_file_path);
        let file = std::fs::File::open(audio_file_path)?;
        /// Send here ACK to Client
        sink.append(rodio::Decoder::try_from(file)?);
        sink.sleep_until_end();
        Ok(())
    }
}
