//! Subscriber actor
//!
//! [`Subscriber`] actor is the transport layer abstraction of the Gateway and Subscriber.

use std::fmt;
use std::net::SocketAddr;

use async_time_mock_tokio::MockableClock;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::{self, Message as TungsteniteMessage};

use tokio_tungstenite::{accept_async, WebSocketStream as TungsteniteWebSocketStream};
use tracing::{debug, error, info, instrument, trace, warn};

use thiserror::Error;
use tokio::io;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use lamarrs_utils::enums::{
    GatewayError, GatewayMessage, RegisterResult, RelativeLocation, Service, SubscriberMessage,
};
use uuid::Uuid;

use crate::services::payload::SusbcriptionData;
use crate::services::text_streamers::{ColorMessage, SubtitleMessage};

#[derive(Debug, thiserror::Error)]
pub enum SuscriberError {
    #[error("An error ocurred while trying to bind the TCP Listener")]
    Error(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct SubscriberId {
    addr: SocketAddr,
    uuid: Option<Uuid>, // By now, each new connection will have a new UUID, later the client must identify itself.
    location: Option<RelativeLocation>,
}

impl SubscriberId {
    pub fn new(addr: SocketAddr, uuid: Option<Uuid>, location: Option<RelativeLocation>) -> Self {
        Self {
            addr,
            uuid,
            location,
        }
    }

    pub fn unregistered(self) -> bool {
        match (self.uuid, self.location) {
            (Some(_), Some(_)) => false,
            (_, _) => true,
        }
    }
}

impl fmt::Display for SubscriberId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Use `self.number` to refer to each positional data point.
        match (self.uuid, self.location.clone()) {
            (None, _) => write!(f, "(Unregistered Subscriber, {})", self.addr),
            (Some(uuid), None) => {
                write!(f, "(id:{}, location: Unknown, address:{})", uuid, self.addr)
            }
            (Some(uuid), Some(location)) => write!(
                f,
                "(id:{}, location:{}, address:{})",
                uuid, location, self.addr
            ),
        }
    }
}

pub struct Subscriber {
    subtitles: Sender<SubtitleMessage>,
    color: Sender<ColorMessage>,

    id: SubscriberId,
    sender: Sender<GatewayMessage>,
    inbox: Receiver<GatewayMessage>,

    clock: MockableClock,
}

impl Subscriber {
    pub fn new(
        subtitles: Sender<SubtitleMessage>,
        color: Sender<ColorMessage>,
        addr: SocketAddr,
    ) -> Self {
        info!("New subscriber is being created: {}", addr);
        let (sender, inbox) = channel(32);
        let subscriber_id = SubscriberId::new(addr, None, None);
        Self {
            subtitles,
            color,
            id: subscriber_id,
            sender,
            inbox,
            clock: MockableClock::Real,
        }
    }

    /// Allows to set the internal clock, used by tests to replace the real one with a mock
    #[cfg(test)]
    pub fn set_clock(&mut self, clock: MockableClock) {
        self.clock = clock
    }

    #[instrument(name = "Subscriber::accept_and_connect", skip(self), fields(url=?self.id.uuid.clone()), level = "INFO")]
    async fn accept_and_connect(
        &self,
        stream: TcpStream,
    ) -> Result<
        (
            SplitSink<TungsteniteWebSocketStream<TcpStream>, TungsteniteMessage>,
            SplitStream<TungsteniteWebSocketStream<TcpStream>>,
        ),
        WebSocketError,
    > {
        debug!("Opening websocket connection.");
        match accept_async(stream).await {
            Ok(ws_stream) => {
                info!("WebSocket connection established: {}", self.id.addr);
                Ok(ws_stream.split())
            }
            Err(_) => Err(WebSocketError::FailedToConnectWithSubscriber),
        }
    }

    pub async fn run(&mut self, stream: TcpStream) -> Result<(), WebSocketError> {
        debug!("Incoming TCP connection from: {}", self.id.addr);
        let (mut outgoing, mut incoming) = self.accept_and_connect(stream).await?;

        loop {
            tokio::select! {
                // Receive messages from websocket connection
                msg = incoming.next() => {
                    debug!(?msg, "New message from websocket from {}", self.id);
                    self.handle_ws_message(msg, &mut outgoing).await?;
                }

                // Receive messages from any other actor
                msg = self.inbox.recv() => {
                    info!(?msg, "Sending message via websocket to {}", self.id);
                    if let Some(message) = msg {
                        let _ = outgoing.send(TungsteniteMessage::Text(serde_json::to_string(&message).unwrap())).await;
                    }
                }
            }
        }
    }

    async fn handle_ws_message(
        &mut self,
        msg: Option<Result<tungstenite::Message, tokio_tungstenite::tungstenite::Error>>,
        outgoing: &mut SplitSink<TungsteniteWebSocketStream<TcpStream>, TungsteniteMessage>,
    ) -> Result<(), WebSocketError> {
        match msg {
            // Process inbound messages
            Some(Ok(TungsteniteMessage::Text(payload))) => {
                debug!(payload, "Inbound message from {}:", self.id);
                self.process_payload(payload).await?;
            }
            // Handle ping responses
            Some(Ok(TungsteniteMessage::Ping(data))) => {
                trace!(?data, "Ping");
                outgoing
                    .send(tungstenite::Message::Pong(data.clone()))
                    .await?;
                trace!(?data, "Pong");
            }

            // Handle connection closed
            Some(Ok(TungsteniteMessage::Close(_))) | None => {
                if let Some(Ok(TungsteniteMessage::Close(Some(reason)))) = msg {
                    warn!(%reason, "WebSocket connection closed");
                } else {
                    warn!("WebSocket connection closed. Unknown reason.");
                }
                return Err(WebSocketError::SuscriberConnectionLost);
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

    async fn process_payload(&mut self, payload: String) -> Result<(), InternalError> {
        if self.id.clone().unregistered() {
            match serde_json::from_str(&payload) {
                Ok(SubscriberMessage::Register((id, location))) => {
                    info!(?id, ?location, "Registring new Subscriber {}", self.id);
                    self.id.uuid = Some(id);
                    self.id.location = Some(location);
                    self.sender
                        .send(GatewayMessage::RegisterResult(RegisterResult::Success))
                        .await
                        .unwrap()
                }
                Ok(_) => {
                    info!(?payload, "Message received from {}", self.id);
                    self.sender
                        .send(GatewayMessage::Error(GatewayError::UnregisteredSubscriber))
                        .await
                        .unwrap();
                    return Ok(()); // Err(InternalError::UnregisteredSubscriber);
                }
                Err(_) => todo!(),
            }
        } else {
            match serde_json::from_str(&payload) {
                Ok(SubscriberMessage::Subscribe(service)) => {
                    info!(?service, "Service Subscription request by {}", self.id);

                    match service {
                        Service::Subtitle => {
                            self.subtitles
                                .send(SubtitleMessage::Subscribe(SusbcriptionData {
                                    sender_id: self.id.uuid.unwrap(),
                                    sender: self.sender.clone(),
                                    location: self.id.location.clone().unwrap(),
                                }))
                                .await
                                .unwrap();
                        }
                        Service::Color => {
                            self.color
                                .send(ColorMessage::Subscribe(SusbcriptionData {
                                    sender_id: self.id.uuid.unwrap(),
                                    sender: self.sender.clone(),
                                    location: self.id.location.clone().unwrap(),
                                }))
                                .await
                                .unwrap();
                        }
                    }
                }
                Err(_) => todo!(),
                _ => todo!(),
            }
        }
        Ok(())
    }

    /// Reconnects and recreate inbox
    async fn recreate_inbox(
        &mut self,
        inbox: &mut Receiver<GatewayMessage>,
    ) -> Result<(), WebSocketError> {
        info!("Reconnecting!!");
        // Close inbox, so no new messages can arrive until we reconnect
        inbox.close();
        info!("Trying to recover suscriptions");
        *inbox = self.reset_inbox().await?;
        Ok(())
    }

    /// Resets the inbox for the [`WebSocket`]
    async fn reset_inbox(&mut self) -> Result<Receiver<GatewayMessage>, WebSocketError> {
        let (sender, inbox) = channel(32);
        self.sender = sender;

        self.subtitles
            .send(SubtitleMessage::UpdateSubscription(SusbcriptionData {
                sender_id: self.id.uuid.unwrap(),
                sender: self.sender.clone(),
                location: self.id.location.clone().unwrap(),
            }))
            .await;
        self.color
            .send(ColorMessage::UpdateSubscription(SusbcriptionData {
                sender_id: self.id.uuid.unwrap(),
                sender: self.sender.clone(),
                location: self.id.location.clone().unwrap(),
            }))
            .await;

        Ok(inbox)
    }
}

/// Type that express fatal errors to [`WebSocket`].
#[derive(Debug, Error)]
pub enum WebSocketError {
    #[error("Failed to connect to subscriber")]
    FailedToConnectWithSubscriber,
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
    #[error("Suscriber connection lost")]
    SuscriberConnectionLost,
    #[error("An internal error happened inside the actor")]
    UnregisteredSubscriber(#[from] InternalError),
}

#[derive(Debug, Error)]
pub enum InternalError {
    #[error("Send error: {0}")]
    TungsteniteError(#[from] tungstenite::Error),

    #[error("Invalid message received from an unidentified Subscriber")]
    UnregisteredSubscriber,
}
