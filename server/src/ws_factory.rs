use crate::services::sound_streamers::MidiMessage;
use crate::services::text_streamers::{ColorMessage, SubtitleMessage};
use crate::subscriber::Subscriber;
use tokio::net::TcpListener;
use tokio::{io, sync::mpsc::Sender};

#[derive(Debug, thiserror::Error)]
pub enum ConnManagerError {
    #[error("An error ocurred while trying to bind the TCP Listener")]
    Error(#[from] io::Error),
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
}

pub struct SubscriberBuilder {
    subtitle: Sender<SubtitleMessage>,
    color: Sender<ColorMessage>,
    midi: Sender<MidiMessage>,
}

impl SubscriberBuilder {
    pub fn new(
        subtitle: Sender<SubtitleMessage>,
        color: Sender<ColorMessage>,
        midi: Sender<MidiMessage>,
    ) -> Self {
        Self {
            subtitle,
            color,
            midi,
        }
    }

    pub async fn run(self, listener: TcpListener) {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tokio::spawn({
                        let mut new_subscriber = Subscriber::new(
                            self.subtitle.clone(),
                            self.color.clone(),
                            self.midi.clone(),
                            addr,
                        );
                        async move {
                            new_subscriber.run(stream).await;
                        }
                    });
                }
                Err(error) => {
                    // Retry reconnect in 10 seconds
                    // TODO: exponential backoff?
                    panic!("Connection listener crashed!")
                }
            }
        }
    }
}
