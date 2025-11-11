//! Subscriber actor
//!
//! [`Subscriber`] actor is the transport layer abstraction of the Gateway and Subscriber.

use async_time_mock_tokio::MockableClock;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use lamarrs_utils::action_messages::Event;
use lamarrs_utils::exchange_messages::{AckResult, ExchangeMessage, NackResult};
use postcard::to_allocvec;
use tokio::{
    net::TcpStream,
    time::{timeout, Duration},
};
use tokio_tungstenite::tungstenite::{self, Message as TungsteniteMessage};

use tokio_tungstenite::{accept_async, WebSocketStream as TungsteniteWebSocketStream};
use tracing::{debug, error, info, instrument, warn};

use thiserror::Error;
use tokio::sync::mpsc::{self, channel, Receiver, Sender};

use crate::services::{self, InternalEventMessageServer};
use lamarrs_utils::{ClientIdAndLocation, ErrorDescription, Service};

#[derive(Debug, Error)]
pub enum ClientHandlerError {
    #[error("Failed to connect to subscriber")]
    FailedToConnectWithSubscriber,
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
    #[error("Client connection lost with: {client_id}")]
    ConnectionLost { client_id: String },
    #[error("Valid Action message received from an unregistered Client: {0}")]
    UnregisteredSubscriber(String),
    #[error("Malformed payload received from an unidentified Client: {0}")]
    UnrecognizableMessage(String),
    #[error("Invalid ExchangeMessage received from remote Client: {0}")]
    InvalidExchangeMessage(String),
    #[error("Fatar error deconding a binary ExchangeMessage received from remote Client: {0}")]
    FailureDecodingBinaryExchangeMessage(String),
    #[error("Error sending an ExchangeMessage")]
    SendActionMessageWithSender(
        #[from] mpsc::error::SendError<services::InternalEventMessageServer>,
    ),
    #[error("Error sending an ExchangeMessage")]
    SendExchangeMessage(
        #[from] mpsc::error::SendError<lamarrs_utils::exchange_messages::ExchangeMessage>,
    ),
}

enum ClientWire {
    Binary,
    Text,
}

pub struct Client {
    id: Option<ClientIdAndLocation>,

    subtitles_service: Sender<InternalEventMessageServer>,
    colour_service: Sender<InternalEventMessageServer>,
    playback_service: Sender<InternalEventMessageServer>,
    midi_service: Sender<InternalEventMessageServer>,
    sequencer: Sender<ExchangeMessage>,

    sender: Sender<ExchangeMessage>,
    inbox: Receiver<ExchangeMessage>,
    clock: MockableClock,
    watchdog_sent: bool,
    wire: ClientWire,
}

impl Client {
    pub fn new(
        subtitles_service: Sender<InternalEventMessageServer>,
        colour_service: Sender<InternalEventMessageServer>,
        playback_service: Sender<InternalEventMessageServer>,
        midi_service: Sender<InternalEventMessageServer>,
        sequencer: Sender<ExchangeMessage>,
    ) -> Self {
        let (sender, inbox) = channel(32);
        let subscriber_id = None;
        Self {
            id: subscriber_id,
            subtitles_service,
            colour_service,
            playback_service,
            midi_service,
            sequencer,
            sender,
            inbox,
            clock: MockableClock::Real,
            watchdog_sent: false,
            wire: ClientWire::Binary,
        }
    }

    /// Allows to set the internal clock, used by tests to replace the real one with a mock.
    #[cfg(test)]
    pub fn set_clock(&mut self, clock: MockableClock) {
        self.clock = clock
    }

    /// Main task of the Client Handler.
    /// This process the remote client request to upgrade and then loops
    /// listening for incoming messages from the remote client or from the
    /// other Actors.
    #[instrument(name = "Client::run", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    pub async fn run(&mut self, stream: TcpStream) -> Result<(), ClientHandlerError> {
        // Creates the Sink and Stream.
        let (mut remote_sender, mut remote_inbox) = self.accept_and_connect(stream).await?;
        let connection_watchdog_timer = Duration::from_hours(5);

        loop {
            tokio::select! {
                // Receive messages from the remote Client via websocket connection
                maybe_message = timeout(connection_watchdog_timer, remote_inbox.next()) => {
                    match maybe_message {
                        Ok(msg) => {
                            info!(?msg, "New message from remote Client via websocket");
                            // TODO: if 3 UnregisteredSubscriber messages are received, terminate the connection.
                            self.handle_ws_message(msg, &mut remote_sender).await?;
                        }
                        Err(_) => {
                            warn!("Watchdog state: {:?}", self.watchdog_sent);
                            if self.watchdog_sent {
                                error!("{:?} is irresponsive, proceeding to close the connection", self.id);
                                if let Some(client_id) = &self.id {
                                    self.subtitles_service.send(InternalEventMessageServer::RemoveTargetClient(client_id.clone())).await?;
                                    self.colour_service.send(InternalEventMessageServer::RemoveTargetClient(client_id.clone())).await?;
                                    self.playback_service.send(InternalEventMessageServer::RemoveTargetClient(client_id.clone())).await?;
                                    self.midi_service.send(InternalEventMessageServer::RemoveTargetClient(client_id.clone())).await?;
                                }
                                break Err(ClientHandlerError::ConnectionLost { client_id: format!("{:?}", self.id) })
                            }
                            match self.wire {
                                ClientWire::Binary => {
                                    match to_allocvec(&ExchangeMessage::Heartbeat) {
                                        Ok(binary_message) =>  remote_sender.send(TungsteniteMessage::Binary(binary_message.into())).await?,
                                        Err(_) => error!("Heartbeat could not be converted to Binary. Message was not sent.")
                                    }
                                }
                                ClientWire::Text => match serde_json::to_string(&ExchangeMessage::Heartbeat) {
                                        Ok(string_message) => remote_sender.send(TungsteniteMessage::Text(string_message.into())).await?,
                                        Err(_) => error!("Heartbeat could not be converted to Binary. Message was not sent.")
                                }
                            };
                            self.watchdog_sent = true;
                        }
                    }
                }

                // Receive messages from any other actors
                msg = self.inbox.recv() => {
                    info!(?msg, "Sending message to remote Client via websocket");
                    if let Some(message) = msg {
                        match self.wire{
                            ClientWire::Binary => {
                                match to_allocvec(&message) {
                                    Ok(binary_message) =>  remote_sender.send(TungsteniteMessage::Binary(binary_message.into())).await?,
                                    Err(_) => error!("Message {:?} to be relayed to Client could not be converted to Binary. Message was not sent.", message)
                                }
                            }
                            ClientWire::Text => match serde_json::to_string(&message) {
                                    Ok(string_message) => remote_sender.send(TungsteniteMessage::Text(string_message.into())).await?,
                                    Err(_) => error!("Message {:?} to be relayed to Client could not be converted to String. Message was not sent.", message)
                            }
                        }
                    }
                }
            }
        }
    }

    /// This function handles the remote Client requests to upgrade an HTTP connection to a
    /// Websocket one.
    /// If succeeds, returns a Sender and Receiver for the newly created WS connection.
    #[instrument(name = "Client::accept_and_connect", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn accept_and_connect(
        &self,
        stream: TcpStream,
    ) -> Result<
        (
            SplitSink<TungsteniteWebSocketStream<TcpStream>, TungsteniteMessage>,
            SplitStream<TungsteniteWebSocketStream<TcpStream>>,
        ),
        ClientHandlerError,
    > {
        debug!("Opening websocket connection.");
        match accept_async(stream).await {
            Ok(ws_stream) => {
                info!("WebSocket connection established with new remote Client.");
                Ok(ws_stream.split())
            }
            Err(_) => Err(ClientHandlerError::FailedToConnectWithSubscriber),
        }
    }

    /// Function that "peels" the outer layer of the webscoket frame.
    #[instrument(name = "Client::handle_ws_message", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn handle_ws_message(
        &mut self,
        msg: Option<Result<tungstenite::Message, tokio_tungstenite::tungstenite::Error>>,
        outgoing: &mut SplitSink<TungsteniteWebSocketStream<TcpStream>, TungsteniteMessage>,
    ) -> Result<(), ClientHandlerError> {
        match msg {
            // Process inbound messages from devices that send json using serde_json.
            Some(Ok(TungsteniteMessage::Text(string_payload))) => {
                debug!(?string_payload, "Inbound Text Payload");
                let payload_as_str_result = serde_json::from_str(string_payload.as_ref());
                let payload = match payload_as_str_result {
                    Ok(payload) => payload,
                    Err(error) => {
                        error!(
                            ?error,
                            "Malformed message received from unregistered device."
                        );
                        let error_descr =
                            heapless::String::try_from("Unrecognized message: Malformed payload.");
                        match error_descr {
                            Ok(error_descr) => {
                                self.sender.send(ExchangeMessage::Error(ErrorDescription{error_descr})).await?;
                            }
                            Err(_) => error!("Client can't be notified of the error, there was an issue parsing the error message.")
                        }
                        return Err(ClientHandlerError::UnrecognizableMessage(
                            string_payload.to_string(),
                        ));
                    }
                };
                if self.id.is_none() {
                    // From now on, we are only going to talk Text with this client.
                    self.wire = ClientWire::Text;
                    // We must get the client a identifier -and lamarrs version in the future-
                    // to secure the client is sane and "orchesteable".
                    self.on_unregistered_subscriber_message(payload).await?
                } else {
                    // If the client has been already identified and is talkable for this server
                    // we can process its messages safely.
                    self.on_subscriber_message(payload).await?
                }
            }
            // The embedded devices send the data encoded in Bytes using postcard.
            Some(Ok(TungsteniteMessage::Binary(bytes_payload))) => {
                debug!(?bytes_payload, "Inbound Binary Payload");
                let payload =
                    postcard::from_bytes::<ExchangeMessage>(&bytes_payload).map_err(|e| {
                        ClientHandlerError::FailureDecodingBinaryExchangeMessage(e.to_string())
                    })?;
                if self.id.is_none() {
                    // From now on, we are only going to talk Binary with this client.
                    self.wire = ClientWire::Binary;
                    // We must get the client a identifier -and lamarrs version in the future-
                    // to secure the client is sane and "orchesteable".
                    self.on_unregistered_subscriber_message(payload).await?
                } else {
                    // If the client has been already identified and is talkable for this server
                    // we can process its messages safely.
                    self.on_subscriber_message(payload).await?
                }
            }
            // Handle connection closed
            Some(Ok(TungsteniteMessage::Close(_))) | None => {
                if let Some(Ok(TungsteniteMessage::Close(Some(reason)))) = msg {
                    warn!(%reason, "WebSocket connection closed");
                } else {
                    warn!("WebSocket connection closed. Unknown reason.");
                }
                return Err(ClientHandlerError::ConnectionLost {
                    client_id: format!("{:?}", self.id),
                });
            }
            // Ignore unsuported messages
            Some(Ok(message)) => {
                warn!(?message, "Ignoring unsupported WebSocket message");
            }
            // Handle connection errors
            Some(Err(error)) => {
                return Err(error)?;
            }
        };
        Ok(())
    }

    /// If a remote Client didn't register itself, its messages will be processed by
    /// this function. It currently expects the remote Client to present is UUID and
    /// optionally, its location. In the future, the Client lamarrs version will also
    /// be checked here to confirm API compatibility with the remote or reject the Client
    /// if its not compatible or if other validations fail.
    #[instrument(name = "Client::on_unregistered_subscriber_message", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn on_unregistered_subscriber_message(
        &mut self,
        exchange_message: ExchangeMessage,
    ) -> Result<(), ClientHandlerError> {
        match exchange_message {
            ExchangeMessage::Request(Event::Register(client_id_and_location)) => {
                info!("Registering new Client {client_id_and_location:?}");
                self.id = Some(client_id_and_location.clone());
                // Recreate sender in all services the if the client is reconnecting and was already subscribed.
                self.subtitles_service
                    .send(InternalEventMessageServer::UpdateClientData(
                        client_id_and_location.clone(),
                        self.sender.clone(),
                    ))
                    .await?;
                self.colour_service
                    .send(InternalEventMessageServer::UpdateClientData(
                        client_id_and_location.clone(),
                        self.sender.clone(),
                    ))
                    .await?;
                self.playback_service
                    .send(InternalEventMessageServer::UpdateClientData(
                        client_id_and_location.clone(),
                        self.sender.clone(),
                    ))
                    .await?;
                self.midi_service
                    .send(InternalEventMessageServer::UpdateClientData(
                        client_id_and_location.clone(),
                        self.sender.clone(),
                    ))
                    .await?;
                // Confirm success to client
                self.sender
                    .send(ExchangeMessage::Ack(AckResult::Success))
                    .await?;
                Ok(())
            }
            ExchangeMessage::HeartbeatAck => {
                info!("HeartbeatAck received by Unregistered device.");
                self.watchdog_sent = false;
                info!("Watchdog reset: {}", self.watchdog_sent);
                Ok(())
            }
            _ => {
                warn!(
                    ?exchange_message,
                    "Message received from unregistered device."
                );
                self.sender
                    .send(ExchangeMessage::Nack(NackResult::NotSubscribed))
                    .await?;
                Err(ClientHandlerError::UnregisteredSubscriber(
                    exchange_message.to_string(),
                ))
            }
        }
    }

    /// This function process messages from trusted remote Clients that already have been
    /// cleared to be compatible, localizable, and that can be identified.
    #[instrument(name = "Client::on_subscriber_message", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn on_subscriber_message(
        &mut self,
        exchange_message: ExchangeMessage,
    ) -> Result<(), ClientHandlerError> {
        match exchange_message {
            ExchangeMessage::Request(action) => match action {
                Event::SuscribeToService(service, client_id_and_location) => {
                    self.subscribe_to_service(service, client_id_and_location)
                        .await
                }
                Event::UnsubscribeFromService(service, client_id_and_location) => {
                    self.unsubscribe_from_service(service, client_id_and_location)
                        .await
                }
                Event::UpdateLocation(client_id_and_location) => {
                    self.update_location(client_id_and_location).await
                }
                _ => {
                    warn!(?action, "Requested Event by Client {:?} is not supported. Client may be sending Server Event?", self.id);
                    self.sender
                        .send(ExchangeMessage::Nack(NackResult::Failed))
                        .await?;
                    Err(ClientHandlerError::InvalidExchangeMessage(
                        action.to_string(),
                    ))
                }
            },
            ExchangeMessage::HeartbeatAck => {
                info!("HeartbeatAck received by {:?}.", self.id);
                self.watchdog_sent = false;
                info!("Watchdog reset: {}", self.watchdog_sent);
                Ok(())
            }
            ExchangeMessage::NextScene => {
                info!("Requesting moving to the next scene to the Orchestrator");
                Ok(self.sequencer.send(exchange_message).await?)
            }
            ExchangeMessage::RetriggerScene => {
                info!("Requesting retrigger to the current scene to the Orchestrator");
                Ok(self.sequencer.send(exchange_message).await?)
            }
            _ => {
                warn!(
                    ?exchange_message,
                    "Invalid message received from device {:?}.", self.id
                );
                self.sender
                    .send(ExchangeMessage::Nack(NackResult::Failed))
                    .await?;
                Err(ClientHandlerError::InvalidExchangeMessage(
                    exchange_message.to_string(),
                ))
            }
        }
    }

    #[instrument(name = "Client::subscriber_to_service", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn subscribe_to_service(
        &mut self,
        service: Service,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), ClientHandlerError> {
        let message = InternalEventMessageServer::AddTargetClient(
            client_id_and_location,
            self.sender.clone(),
        );
        match service {
            Service::Subtitle => Ok(self.subtitles_service.send(message).await?),
            Service::Colour => Ok(self.colour_service.send(message).await?),
            Service::AudioPlayer => Ok(self.playback_service.send(message).await?),
            Service::Midi => Ok(self.midi_service.send(message).await?)
        }
    }

    #[instrument(name = "Client::unsubscribe_from_service", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn unsubscribe_from_service(
        &mut self,
        service: Service,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), ClientHandlerError> {
        let message = InternalEventMessageServer::RemoveTargetClient(client_id_and_location);
        match service {
            Service::Subtitle=>Ok(self.subtitles_service.send(message).await?),
            Service::Colour=>Ok(self.colour_service.send(message).await?),
            Service::AudioPlayer=>Ok(self.playback_service.send(message).await?),
            Service::Midi => Ok(self.midi_service.send(message).await?)
        }
    }

    #[instrument(name = "Client::update_location", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn update_location(
        &mut self,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), ClientHandlerError> {
        self.subtitles_service
            .send(InternalEventMessageServer::UpdateClientData(
                client_id_and_location.clone(),
                self.sender.clone(),
            ))
            .await?;
        self.colour_service
            .send(InternalEventMessageServer::UpdateClientData(
                client_id_and_location,
                self.sender.clone(),
            ))
            .await?;
        Ok(())
    }
}
