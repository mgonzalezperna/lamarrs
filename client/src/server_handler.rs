use async_time_mock_tokio::MockableClock;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use http::Uri;
use lamarrs_utils::{
    action_messages::{Action, Event},
    exchange_messages::{ExchangeMessage, NackResult},
    ClientIdAndLocation, ErrorDescription, RelativeLocation, Service,
};
use tokio::{
    net::TcpStream,
    sync::mpsc::{self, channel, Receiver, Sender},
};
use tokio_tungstenite::{
    connect_async, tungstenite::Message as TungsteniteMessage, MaybeTlsStream,
    WebSocketStream as TungsteniteWebSocketStream,
};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::InternalEventMessageClient;

#[derive(Debug, thiserror::Error)]
pub enum ServerHandlerError {
    #[error("No websocket server found in the provided URL.")]
    ParseError(#[from] url::ParseError),
    #[error("Connection could't not be stablished with the server.")]
    Error(#[from] tungstenite::error::Error),
    #[error("Server connection lost")]
    ServerConnectionLost,
    #[error("Malformed payload received from Server: {0}")]
    UnrecognizableMessage(String),
    #[error("Invalid ExchangeMessage received from Server: {0}")]
    InvalidExchangeMessage(String),
    #[error("Error sending an InternalMessage to Server handler")]
    SendInternalMessage(#[from] mpsc::error::SendError<InternalEventMessageClient>),
    #[error("Fatar error deconding a binary ExchangeMessage received from Server: {0}")]
    FailureDecodingBinaryExchangeMessage(String),
}

pub struct Client {
    id: Uuid,
    location: Option<RelativeLocation>,
    server_address: Uri,
    sender: Sender<InternalEventMessageClient>,
    inbox: Receiver<InternalEventMessageClient>,
    audio_player: Sender<InternalEventMessageClient>,
    //dmx: Sender<InternalEventMessageClient>,
    //led: Sender<InternalEventMessageClient>,
    //midi: Sender<InternalEventMessageClient>,
    clock: MockableClock,
}

impl Client {
    /// Client Actor constructor.
    pub fn new(
        location: Option<RelativeLocation>,
        server_address: Uri,
        audio_player: Sender<InternalEventMessageClient>,
        // dmx: Sender<InternalEventMessageClient>,
        // led: Sender<InternalEventMessageClient>,
        // midi: Sender<InternalEventMessageClient>,
    ) -> Self {
        let (sender, inbox) = channel(32);
        Self {
            id: Uuid::new_v4(),
            location,
            server_address,
            sender,
            inbox,
            audio_player,
            //subtitle,
            //dmx,
            //led,
            //midi,
            clock: MockableClock::Real,
        }
    }

    /// Main task of the Server Handler.
    /// This request upgrading the connection to the server then loops
    /// listening for incoming messages from the remote client or from the
    /// other Actors.
    #[instrument(name = "Client::run", skip(self), fields(id=?self.id), level = "INFO", ret, err)]
    pub async fn run(&mut self) -> Result<(), ServerHandlerError> {
        let (ws_stream, _) = connect_async(&self.server_address).await?;
        info!(
            "WebSocket handshake with {} has been completed successfully",
            self.server_address.to_string()
        );

        let (mut remote_sender, mut remote_inbox) = ws_stream.split();

        // We add to the queue the request to Register to the Server.
        let register_message = ExchangeMessage::Request(Event::Register(ClientIdAndLocation {
            uuid: self.id,
            location: self.location.clone(),
        }));
        match serde_json::to_string(&register_message) {
            // TODO: Loop and don´t progress unless ACK is received. Otherwise, panic?
            Ok(string_message) => remote_sender.send(TungsteniteMessage::Text(string_message.into())).await?,
            Err(_) => error!("Message {:?} to be relayed to Client {:?} could not be converted to String. Message was not sent.", register_message, self.id)
        }
        /// Notify internal services that all systems are go. Services will answer with Subscribe requests.
        /// We should also process ACK, but that means refactoring the receiver. Will be done later.
        self.audio_player
            .send(InternalEventMessageClient::ConnectedToServer(
                self.sender.clone(),
            ))
            .await?;

        loop {
            tokio::select! {
                // Receive messages from server via websocket connection
                msg = remote_inbox.next() => {
                    info!(?msg, "New message from server via websocket");
                    // TODO: if 3 UnregisteredSubscriber messages are received, terminate the connection.
                    if let Err(error) = self.handle_ws_message(msg, &mut remote_sender).await {
                        // Some errors must stop the loop, others just inform.
                        match error {
                            ServerHandlerError::ParseError(parse_error)=>(),
                            ServerHandlerError::Error(error)=>(),
                            ServerHandlerError::ServerConnectionLost=>(),
                            ServerHandlerError::UnrecognizableMessage(_)=>(),
                            ServerHandlerError::InvalidExchangeMessage(_)=>(),
                            ServerHandlerError::SendInternalMessage(send_error)=>(),
                            ServerHandlerError::FailureDecodingBinaryExchangeMessage(_) =>(),
                        }
                    }
                }
                // Receive messages from any other actors
                msg = self.inbox.recv() => {
                    info!(?msg, "Sending message to server via websocket");
                    if let Some(message) = msg {
                        /// Services don't have the ID client data and neither shoud have it.
                        /// The Server Handler will create the ExchangeMessage frames adding over the Actor internal request.
                        /// Perhaps this API could be streamlined more...
                        match message {
                            InternalEventMessageClient::SubscribeToService(service) => {
                                let exchange_message = ExchangeMessage::Request(Event::SuscribeToService(service, ClientIdAndLocation::new(self.id, self.location.clone())));
                                match serde_json::to_string(&exchange_message) {
                                    Ok(string_message) => remote_sender.send(TungsteniteMessage::Text(string_message.into())).await?,
                                    Err(_) => error!("Message {:?} to be relayed to Client {:?} could not be converted to String. Message was not sent.", exchange_message, self.id)
                                }
                            }
                            _ => { error!("Invalid message type received from internal actor.") }
                        }
                    }
                }
            }
        }
    }

    /// Function that "peels" the outer layer of the webscoket frame.
    #[instrument(name = "Client::handle_ws_message", skip(self),fields(id=?self.id),  level = "INFO")]
    async fn handle_ws_message(
        &mut self,
        msg: Option<Result<tungstenite::Message, tokio_tungstenite::tungstenite::Error>>,
        outgoing: &mut SplitSink<
            TungsteniteWebSocketStream<MaybeTlsStream<TcpStream>>,
            TungsteniteMessage,
        >,
    ) -> Result<(), ServerHandlerError> {
        match msg {
            // Process inbound messages
            Some(Ok(TungsteniteMessage::Text(string_payload))) => {
                debug!(?string_payload, "Inbound String Payload");
                self.process_message(string_payload.to_string()).await?
            }
            Some(Ok(TungsteniteMessage::Binary(bytes_payload))) => {
                debug!(?bytes_payload, "Inbound Binary Payload");
                let payload =
                    postcard::from_bytes::<ExchangeMessage>(&bytes_payload).map_err(|e| {
                        ServerHandlerError::FailureDecodingBinaryExchangeMessage(e.to_string())
                    })?;
                self.process_message(payload.to_string()).await?
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
                return Err(ServerHandlerError::ServerConnectionLost);
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

    /// This function process messages from trusted remote Clients that already have been
    /// cleared to be compatible, localizable, and that can be identified.
    #[instrument(name = "Client::process_message", skip(self),fields(id=?self.id),  level = "INFO", ret, err)]
    async fn process_message(
        &mut self,
        exchange_message: String,
    ) -> Result<(), ServerHandlerError> {
        match serde_json::from_str(&exchange_message) {
            Ok(ExchangeMessage::Scene(Event::PerformAction(action))) => match action {
                Action::ShowNewSubtitles(subtitles) => todo!(),
                Action::ChangeColour(colour_rgb) => todo!(),
                Action::PlayAudio(audio_file) => Ok(self
                    .audio_player
                    .send(InternalEventMessageClient::PlayAudio(
                        audio_file,
                        self.sender.clone(),
                    ))
                    .await?),
            },
            Ok(ExchangeMessage::Request(_)) => {
                warn!(?exchange_message, "Requested Action by Server is not supported. Server may be sending Client Actions?");
                // self.sender
                //     .send(ExchangeMessage::Nack(NackResult::Failed))
                //     .await;
                Err(ServerHandlerError::InvalidExchangeMessage(exchange_message))
            }
            Ok(_) => {
                warn!(
                    ?exchange_message,
                    "Invalid message received from server {:?}.", self.id
                );
                // self.sender
                //     .send(ExchangeMessage::Nack(NackResult::Failed))
                //     .await;
                Err(ServerHandlerError::InvalidExchangeMessage(exchange_message))
            }
            Err(_) => {
                error!(
                    ?exchange_message,
                    "Malformed message received from server {:?}.", self.id
                );
                let error_descr =
                    heapless::String::try_from("Unrecognized message: Malformed payload.");
                match error_descr {
                    Ok(error_descr) => {
                        ExchangeMessage::Error(ErrorDescription{error_descr}); //This is here just to help with the error_descr field type inference.
                        //self.sender.send(ExchangeMessage::Error(ErrorDescription{error_descr})).await;
                    }
                    Err(_) => error!("Client can't be notified of the error, there was an issue parsing the error message."),
                };
                Err(ServerHandlerError::UnrecognizableMessage(exchange_message))
            }
        }
    }
}
