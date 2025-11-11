use crate::client_handler::Client;
use crate::services::InternalEventMessageServer;
use color_eyre::eyre::eyre;
use lamarrs_utils::exchange_messages::ExchangeMessage;
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;
use tracing::info;

/// The ClientBuilder holds copies of Senders to all the Actors in order to provide the different Clients
/// with Senders before spinning them up.
pub struct ClientBuilder {
    subtitle: Sender<InternalEventMessageServer>,
    color: Sender<InternalEventMessageServer>,
    playback: Sender<InternalEventMessageServer>,
    midi: Sender<InternalEventMessageServer>,
    sequencer: Sender<ExchangeMessage>,
}

impl ClientBuilder {
    /// ClientBuilder Actor constructor.
    pub fn new(
        subtitle: Sender<InternalEventMessageServer>,
        color: Sender<InternalEventMessageServer>,
        playback: Sender<InternalEventMessageServer>,
        midi: Sender<InternalEventMessageServer>,
        sequencer: Sender<ExchangeMessage>,
    ) -> Self {
        Self {
            subtitle,
            color,
            playback,
            midi,
            sequencer,
        }
    }

    /// Listen to incoming TCP connections.
    /// If the connection is successful, it creates a Client Actor and runs it as a new Async Task.
    ///
    /// Important!!
    /// The WebSocket is not being created here. That is being handled by the Client.
    pub async fn run(self, listener: TcpListener) {
        loop {
            match listener.accept().await {
                Ok((stream, socket_addr)) => {
                    // For each new TCP connection, we spawn a new task and return to listen for incoming connections.
                    tokio::spawn({
                        info!("Creating new Client: {}", socket_addr);
                        let mut new_client = Client::new(
                            self.subtitle.clone(),
                            self.color.clone(),
                            self.playback.clone(),
                            self.midi.clone(),
                            self.sequencer.clone(),
                        );
                        async move {
                            info!("Starting new Client handler: {}", socket_addr);
                            // Here is where the WS upgrade request will be handled.
                            new_client.run(stream).await;
                        }
                    });
                }
                Err(error) => {
                    // Report a failed connection attempt and loop.
                    tracing::error!("{}", eyre!("Connection listener crashed! {}", error))
                }
            }
        }
    }
}
