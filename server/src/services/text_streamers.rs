use lamarrs_utils::{
    enums::{Color, GatewayMessage, RelativeLocation, SubscribeResult},
    messages::{SendColor, SendSubtitle, Subtitle},
};
use std::collections::{hash_map, HashMap};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::{debug, error, field::debug, info, instrument, trace, warn};
use uuid::Uuid;

use super::payload::SusbcriptionData;

#[derive(Debug, thiserror::Error)]
pub enum TextStreamerError {
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
}

/// Messages [`SubtitleStreamer`] operates on.
#[derive(Debug)]
pub enum SubtitleMessage {
    Subscribe(SusbcriptionData),
    UpdateSubscription(SusbcriptionData),
    SendSubtitle(SendSubtitle),
}

/// Messages [`ColorStreamer`] operates on.
#[derive(Debug)]
pub enum ColorMessage {
    Subscribe(SusbcriptionData),
    UpdateSubscription(SusbcriptionData),
    SendColor(SendColor),
}

pub struct SubtitlesStreamer {
    targets: HashMap<Uuid, (Sender<GatewayMessage>, RelativeLocation)>,
    pub sender: Sender<SubtitleMessage>,
    receiver: Receiver<SubtitleMessage>,
}

impl SubtitlesStreamer {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            targets: HashMap::new(),
            sender,
            receiver,
        }
    }

    pub async fn subscribe(
        &mut self,
        sender_id: Uuid,
        sender: Sender<GatewayMessage>,
        location: RelativeLocation,
    ) {
        debug!(?sender_id, "Processing subscription request");
        let subscribe_result = match self.targets.entry(sender_id) {
            hash_map::Entry::Occupied(_) => SubscribeResult::AlreadySubscribed,
            hash_map::Entry::Vacant(_) => {
                self.targets
                    .insert(sender_id, (sender.clone(), location.clone()));
                SubscribeResult::Success
            }
        };
        sender
            .send(GatewayMessage::SubscribeResult(subscribe_result))
            .await;
    }

    pub async fn update_subscription(
        &mut self,
        sender_id: Uuid,
        sender: Sender<GatewayMessage>,
        location: RelativeLocation,
    ) {
        let update_result = match self.targets.entry(sender_id) {
            hash_map::Entry::Occupied(mut occupied_entry) => {
                let entry = occupied_entry.get_mut();
                *entry = (sender.clone(), location.clone());
                SubscribeResult::UpdatedSubscription
            }
            hash_map::Entry::Vacant(_) => SubscribeResult::NotSubscribed,
        };
        sender
            .send(GatewayMessage::SubscribeResult(update_result))
            .await;
    }

    pub async fn send(&mut self, message: Subtitle, target_location: RelativeLocation) {
        for sender in self.get_senders_by_location(target_location) {
            sender.send(GatewayMessage::Subtitle(message.clone())).await;
        }
    }

    fn get_senders_by_location(
        &mut self,
        target_location: RelativeLocation,
    ) -> Vec<Sender<GatewayMessage>> {
        self.targets
            .clone()
            .into_iter()
            .filter_map(|(_, (sender, location))| {
                if location == target_location {
                    Some(sender)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    pub async fn run(&mut self) -> Result<(), TextStreamerError> {
        loop {
            while let Some(message) = self.receiver.recv().await {
                match message {
                    SubtitleMessage::Subscribe(details) => {
                        self.subscribe(details.sender_id, details.sender, details.location)
                            .await
                    }
                    SubtitleMessage::UpdateSubscription(details) => {
                        self.update_subscription(
                            details.sender_id,
                            details.sender,
                            details.location,
                        )
                        .await
                    }
                    SubtitleMessage::SendSubtitle(subtitle) => {
                        self.send(subtitle.subtitle, subtitle.target_location).await
                    }
                    _ => panic!("Unsupported message type received"),
                }
            }
        }
    }
}

pub struct ColorStreamer {
    targets: HashMap<Uuid, (Sender<GatewayMessage>, RelativeLocation)>,
    pub sender: Sender<ColorMessage>,
    receiver: Receiver<ColorMessage>,
}

impl ColorStreamer {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            targets: HashMap::new(),
            sender,
            receiver,
        }
    }

    pub async fn subscribe(
        &mut self,
        sender_id: Uuid,
        sender: Sender<GatewayMessage>,
        location: RelativeLocation,
    ) {
        debug!(?sender_id, "Processing subscription request");
        let subscribe_result = match self.targets.entry(sender_id) {
            hash_map::Entry::Occupied(_) => SubscribeResult::AlreadySubscribed,
            hash_map::Entry::Vacant(_) => {
                self.targets
                    .insert(sender_id, (sender.clone(), location.clone()));
                SubscribeResult::Success
            }
        };
        sender
            .send(GatewayMessage::SubscribeResult(subscribe_result))
            .await;
        self.send(Color::Blue, RelativeLocation::Center).await;
    }

    pub async fn update_subscription(
        &mut self,
        sender_id: Uuid,
        sender: Sender<GatewayMessage>,
        location: RelativeLocation,
    ) {
        let update_result = match self.targets.entry(sender_id) {
            hash_map::Entry::Occupied(mut occupied_entry) => {
                let entry = occupied_entry.get_mut();
                *entry = (sender.clone(), location.clone());
                SubscribeResult::UpdatedSubscription
            }
            hash_map::Entry::Vacant(_) => SubscribeResult::NotSubscribed,
        };
        sender
            .send(GatewayMessage::SubscribeResult(update_result))
            .await;
    }

    pub async fn send(&mut self, color: Color, target_location: RelativeLocation) {
        for sender in self.get_senders_by_location(target_location) {
            sender.send(GatewayMessage::Color(color.clone())).await;
        }
    }

    fn get_senders_by_location(
        &mut self,
        target_location: RelativeLocation,
    ) -> Vec<Sender<GatewayMessage>> {
        self.targets
            .clone()
            .into_iter()
            .filter_map(|(_, (sender, location))| {
                if location == target_location {
                    Some(sender)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    pub async fn run(&mut self) -> Result<(), TextStreamerError> {
        loop {
            while let Some(message) = self.receiver.recv().await {
                debug!("Processing new message");
                match message {
                    ColorMessage::Subscribe(details) => {
                        self.subscribe(details.sender_id, details.sender, details.location)
                            .await
                    }
                    ColorMessage::UpdateSubscription(details) => {
                        self.subscribe(details.sender_id, details.sender, details.location)
                            .await
                    }
                    ColorMessage::SendColor(color) => {
                        self.send(color.color, color.target_location).await
                    }
                }
            }
        }
        panic!("Unsupported message type received")
    }
}
