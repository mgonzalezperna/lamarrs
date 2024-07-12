use lamarrs_utils::{
    enums::{Color, GatewayMessage, RelativeLocation, SubscribeResult},
    messages::{SendColor, SendMidiEvent, SendSubtitle, Subtitle},
    midi_event::MidiEvent,
};
use std::{
    collections::{hash_map, HashMap},
    fmt,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::{debug, error, field::debug, info, instrument, trace, warn};
use uuid::Uuid;

use super::payload::SusbcriptionData;

#[derive(Debug, thiserror::Error)]
pub enum SoundStreamerError {
    #[error("Error during the websocket handshake occurred")]
    WsError(#[from] tungstenite::error::Error),
}

/// Messages [`MIDIStreamer`] operates on.
#[derive(Debug)]
pub enum MidiMessage {
    Subscribe(SusbcriptionData),
    UpdateSubscription(SusbcriptionData),
    SendMidi(SendMidiEvent),
}

pub struct MidiStreamer {
    targets: HashMap<Uuid, (Sender<GatewayMessage>, RelativeLocation)>,
    pub sender: Sender<MidiMessage>,
    receiver: Receiver<MidiMessage>,
}

impl MidiStreamer {
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
        info!(?sender_id, "MIDI subscription update for");
        let update_result = match self.targets.entry(sender_id) {
            hash_map::Entry::Occupied(mut occupied_entry) => {
                info!(?sender_id, "Found entry for");
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

    pub async fn send(&mut self, message: MidiEvent, target_location: RelativeLocation) {
        for sender in self.get_senders_by_location(target_location) {
            sender.send(GatewayMessage::Midi(message.clone())).await;
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

    pub async fn run(&mut self) -> Result<(), SoundStreamerError> {
        loop {
            while let Some(message) = self.receiver.recv().await {
                match message {
                    MidiMessage::Subscribe(details) => {
                        self.subscribe(details.sender_id, details.sender, details.location)
                            .await
                    }
                    MidiMessage::UpdateSubscription(details) => {
                        self.update_subscription(
                            details.sender_id,
                            details.sender,
                            details.location,
                        )
                        .await
                    }
                    MidiMessage::SendMidi(event) => {
                        self.send(event.event, event.target_location).await
                    }
                    _ => panic!("Unsupported message type received"),
                }
            }
        }
    }
}
