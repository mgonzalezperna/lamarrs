//! Subscriber actor
//!
//! [`Subscriber`] actor is the transport layer abstraction of the Gateway and Subscriber.

use async_time_mock_tokio::MockableClock;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use lamarrs_utils::action_messages::Event;
use lamarrs_utils::exchange_messages::{AckResult, ExchangeMessage, NackResult};
use tokio::net::TcpStream;
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
    #[error("Suscriber connection lost")]
    SuscriberConnectionLost,
    #[error("Valid Action message received from an unregistered Client: {0}")]
    UnregisteredSubscriber(String),
    #[error("Malformed payload received from an unidentified Client: {0}")]
    UnrecognizableMessage(String),
    #[error("Invalid ExchangeMessage received from remote Client: {0}")]
    InvalidExchangeMessage(String),
    #[error("Error sending an ExchangeMessage")]
    SendActionMessageWithSender(#[from] mpsc::error::SendError<services::InternalEventMessageServer>),
}

pub struct Client {
    id: Option<ClientIdAndLocation>,

    subtitles_service: Sender<InternalEventMessageServer>,
    colour_service: Sender<InternalEventMessageServer>,
    playback_service: Sender<InternalEventMessageServer>,

    sender: Sender<ExchangeMessage>,
    inbox: Receiver<ExchangeMessage>,
    clock: MockableClock,
}

impl Client {
    pub fn new(
        subtitles_service: Sender<InternalEventMessageServer>,
        colour_service: Sender<InternalEventMessageServer>,
        playback_service: Sender<InternalEventMessageServer>,
    ) -> Self {
        let (sender, inbox) = channel(32);
        let subscriber_id = None;
        Self {
            id: subscriber_id,
            subtitles_service,
            colour_service,
            playback_service,
            sender,
            inbox,
            clock: MockableClock::Real,
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

        loop {
            tokio::select! {
                // Receive messages from the remote Client via websocket connection
                msg = remote_inbox.next() => {
                    info!(?msg, "New message from remote Client via websocket");
                    // TODO: if 3 UnregisteredSubscriber messages are received, terminate the connection.
                    self.handle_ws_message(msg, &mut remote_sender).await?;
                }

                // Receive messages from any other actors
                msg = self.inbox.recv() => {
                    info!(?msg, "Sending message to remote Client via websocket");
                    if let Some(message) = msg {
                        match serde_json::to_string(&message) {
                            Ok(string_message) => remote_sender.send(TungsteniteMessage::Text(string_message.into())).await?,
                            Err(_) => error!("Message {:?} to be relayed to Client could not be converted to String. Message was not sent.", message)
                        }
                    }
                }
            }
        }
    }

    /// This function handles the remote Client requests to upgrade an HTTP connection to a
    /// Websocket one.
    /// If succeeds, returns a Sender and Receiver for the newly created WS connection.
    // #[instrument(name = "Subscriber::accept_and_connect", skip(self), fields(url=?self.id.uuid.clone()), level = "INFO")]
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
    #[instrument(name = "Client::handle_ws_message", skip(self), fields(id=?self.id), level = "INFO")]
    async fn handle_ws_message(
        &mut self,
        msg: Option<Result<tungstenite::Message, tokio_tungstenite::tungstenite::Error>>,
        outgoing: &mut SplitSink<TungsteniteWebSocketStream<TcpStream>, TungsteniteMessage>,
    ) -> Result<(), ClientHandlerError> {
        match msg {
            // Process inbound messages
            Some(Ok(TungsteniteMessage::Text(string_payload))) => {
                debug!(?string_payload, "Inbound Text Message");
                if let None = self.id {
                    // We must get the client a identifier -and lamarrs version in the future-
                    // to secure the client is sane and "orchesteable".
                    self.on_unregistered_subscriber_message(string_payload.to_string())
                        .await?
                } else {
                    // If the client has been already identified and is talkable for this server
                    // we can process its messages safely.
                    self.on_subscriber_message(string_payload.to_string())
                        .await?
                }
            }
            // Handle ping responses
            Some(Ok(TungsteniteMessage::Ping(data))) => {
                debug!(?data, "Ping");
                outgoing
                    .send(tungstenite::Message::Pong(data.clone()))
                    .await?;
                debug!(?data, "Pong");
            }
            // Handle connection closed
            Some(Ok(TungsteniteMessage::Close(_))) | None => {
                if let Some(Ok(TungsteniteMessage::Close(Some(reason)))) = msg {
                    warn!(%reason, "WebSocket connection closed");
                } else {
                    warn!("WebSocket connection closed. Unknown reason.");
                }
                return Err(ClientHandlerError::SuscriberConnectionLost);
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
        exchange_message: String,
    ) -> Result<(), ClientHandlerError> {
        match serde_json::from_str(&exchange_message) {
            Ok(ExchangeMessage::Request(Event::Register(client_id_and_location))) => {
                info!("Registring new Client {client_id_and_location:?}");
                self.id = Some(client_id_and_location);
                self.sender
                    .send(ExchangeMessage::Ack(AckResult::Success))
                    .await;
                Ok(())
            }
            Ok(_) => {
                warn!(
                    ?exchange_message,
                    "Message received from unregistered device."
                );
                self.sender
                    .send(ExchangeMessage::Nack(NackResult::NotSubscribed))
                    .await;
                Err(ClientHandlerError::UnregisteredSubscriber(exchange_message))
            }
            Err(_) => {
                error!(
                    ?exchange_message,
                    "Malformed message received from unregistered device."
                );
                let error_descr =
                    heapless::String::try_from("Unrecognized message: Malformed payload.");
                match error_descr {
                    Ok(error_descr) => {
                        self.sender.send(ExchangeMessage::Error(ErrorDescription{error_descr})).await;
                    }
                    Err(_) => error!("Client can't be notified of the error, there was an issue parsing the error message.")
                }
                Err(ClientHandlerError::UnrecognizableMessage(exchange_message))
            }
        }
    }

    /// This function process messages from trusted remote Clients that already have been
    /// cleared to be compatible, localizable, and that can be identified.
    #[instrument(name = "Client::on_subscriber_message", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn on_subscriber_message(
        &mut self,
        exchange_message: String,
    ) -> Result<(), ClientHandlerError> {
        match serde_json::from_str(&exchange_message) {
            Ok(ExchangeMessage::Request(action)) => match action {
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
                    warn!(?exchange_message, "Requested Action by Client {:?} is not supported. Client may be sending Server Actions?", self.id);
                    self.sender
                        .send(ExchangeMessage::Nack(NackResult::Failed))
                        .await;
                    Err(ClientHandlerError::InvalidExchangeMessage(exchange_message))
                }
            },
            Ok(_) => {
                warn!(
                    ?exchange_message,
                    "Invalid message received from device {:?}.", self.id
                );
                self.sender
                    .send(ExchangeMessage::Nack(NackResult::Failed))
                    .await;
                Err(ClientHandlerError::InvalidExchangeMessage(exchange_message))
            }
            Err(_) => {
                error!(
                    ?exchange_message,
                    "Malformed message received from device {:?}.", self.id
                );
                let error_descr =
                    heapless::String::try_from("Unrecognized message: Malformed payload.");
                match error_descr {
                    Ok(error_descr) => {
                        self.sender.send(ExchangeMessage::Error(ErrorDescription{error_descr})).await;
                    }
                    Err(_) => error!("Client can't be notified of the error, there was an issue parsing the error message."),
                };
                Err(ClientHandlerError::UnrecognizableMessage(exchange_message))
            }
        }
    }

    #[instrument(name = "Client::subscriber_to_service", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn subscribe_to_service(
        &mut self,
        service: Service,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), ClientHandlerError> {
        let message =
            InternalEventMessageServer::AddTargetClient(client_id_and_location, self.sender.clone());
        match service {
            Service::Subtitle => Ok(self.subtitles_service.send(message).await?),
            Service::Colour => Ok(self.colour_service.send(message).await?),
            Service::AudioPlayer => Ok(self.playback_service.send(message).await?),
        }
    }

    #[instrument(name = "Client::unsubscribe_from_service", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn unsubscribe_from_service(
        &mut self,
        service: Service,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), ClientHandlerError> {
        let message = InternalEventMessageServer::RemoveTargetClient(
            client_id_and_location,
            self.sender.clone(),
        );
        match service {
            Service::Subtitle => Ok(self.subtitles_service.send(message).await?),
            Service::Colour => Ok(self.colour_service.send(message).await?),
            Service::AudioPlayer => Ok(self.playback_service.send(message).await?),
        }
    }

    #[instrument(name = "Client::update_location", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    async fn update_location(
        &mut self,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), ClientHandlerError> {
        self.subtitles_service
            .send(InternalEventMessageServer::UpdateLocation(
                client_id_and_location.clone(),
                self.sender.clone(),
            ))
            .await?;
        self.colour_service
            .send(InternalEventMessageServer::UpdateLocation(
                client_id_and_location,
                self.sender.clone(),
            ))
            .await?;
        Ok(())
    }

    /// Reconnects and recreate inbox.
    /// This is vestigial from previous implementations and will be recycled
    /// later when working on recovery mechanisms.
    async fn recreate_inbox(
        &mut self,
        inbox: &mut Receiver<ExchangeMessage>,
    ) -> Result<(), ClientHandlerError> {
        info!("Reconnecting!!");
        // Close inbox, so no new messages can arrive until we reconnect
        inbox.close();
        info!("Trying to recover suscriptions");
        *inbox = self.reset_inbox().await?;
        Ok(())
    }

    /// Resets the inbox for the [`WebSocket`]
    /// This is vestigial from previous implementations and will be recycled
    /// later when working on recovery mechanisms.
    async fn reset_inbox(&mut self) -> Result<Receiver<ExchangeMessage>, ClientHandlerError> {
        let (sender, inbox) = channel(32);
        self.sender = sender;

        // TODO: rebuild the senders
        // self.subtitles
        //     .send(SubtitleMessage::UpdateSubscription(SusbcriptionData {
        //         sender_id: self.id.uuid.unwrap(),
        //         sender: self.sender.clone(),
        //         location: self.id.location.clone().unwrap(),
        //     }))
        //     .await;
        // self.color
        //     .send(ColorMessage::UpdateSubscription(SusbcriptionData {
        //         sender_id: self.id.uuid.unwrap(),
        //         sender: self.sender.clone(),
        //         location: self.id.location.clone().unwrap(),
        //     }))
        //     .await;
        // self.midi
        //     .send(MidiMessage::UpdateSubscription(SusbcriptionData {
        //         sender_id: self.id.uuid.unwrap(),
        //         sender: self.sender.clone(),
        //         location: self.id.location.clone().unwrap(),
        //     }))
        //     .await;
        Ok(inbox)
    }
}
