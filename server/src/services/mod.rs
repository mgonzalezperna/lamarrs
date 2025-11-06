pub mod service;

use lamarrs_utils::{
    action_messages::{Event, Action},
    exchange_messages::{AckResult, ExchangeMessage, NackResult},
    ClientIdAndLocation, RelativeLocation,
};
use tokio::sync::mpsc::{self, Sender};
use tracing::info;
use uuid::Uuid;

use std::{
    collections::{hash_map, HashMap},
    fmt::Display,
};

#[derive(Debug, thiserror::Error)]
pub enum LamarrsServiceError {
    #[error("There was a irrecoverable error with the service {}.", service)]
    Service { service: String },
    #[error("Error sending an ExchangeMessage")]
    SendExchangeMessage(
        #[from] mpsc::error::SendError<lamarrs_utils::exchange_messages::ExchangeMessage>,
    ),
    #[error("The message type {} requested to send to the Target Clients is not allowed for the service {}.", action_message, service)]
    NotAllowedMessageType {
        action_message: String,
        service: String,
    },
}


/// This is a distorted re-export of the ActionMessage from utils.
/// Moving this to the lamarrs-utils doesn't make sense right now because
///  a- It is not needed in any other workspace
///  b- Would force lamarrs-utils to have Tokio as dependency, which would break web-ui as Tokio doesn't play nice with WASM and no-std.
/// TODO: Add these to `utils` and gate them under a feature flag: https://stackoverflow.com/questions/75599346/what-are-the-consequences-of-a-feature-gated-enum-variant
/// Internal message types to be transmited between actors inside Lamarrs server.
/// These are also the payloads the clients will be sending inside the Exchange Messages.
#[derive(Debug)]
pub enum InternalEventMessageServer {
    AddTargetClient(ClientIdAndLocation, Sender<ExchangeMessage>),
    RemoveTargetClient(ClientIdAndLocation, Sender<ExchangeMessage>),
    UpdateLocation(ClientIdAndLocation, Sender<ExchangeMessage>),
    UpdateClients(Action, Option<RelativeLocation>),
}

#[derive(Debug)]
pub struct TargetClient {
    sender: Sender<ExchangeMessage>,
    location: Option<RelativeLocation>,
}

pub trait LamarrsService: Display {
    fn action_is_allowed(&self, message: &Action) -> bool;
    fn get_target_client_map(&mut self) -> &mut HashMap<Uuid, TargetClient>;
    async fn receive_message(&mut self) -> Option<InternalEventMessageServer>;

    /// Runs the Service.
    async fn run(&mut self) -> Result<(), LamarrsServiceError> {
        loop {
            while let Some(message) = self.receive_message().await {
                match message {
                    InternalEventMessageServer::AddTargetClient(
                        client_id_and_location,
                        client_sender,
                    )
                    | InternalEventMessageServer::UpdateLocation(
                        client_id_and_location,
                        client_sender,
                    ) => {
                        self.upsert_target_client(client_id_and_location, client_sender)
                            .await?
                    }
                    InternalEventMessageServer::RemoveTargetClient(
                        client_id_and_location,
                        client_sender,
                    ) => {
                        self.remove_target_client(client_id_and_location, client_sender)
                            .await?
                    }
                    InternalEventMessageServer::UpdateClients(
                        message_for_subscribed_clients,
                        relative_location,
                    ) => {
                        self.write_to_target_clients(
                            message_for_subscribed_clients,
                            relative_location,
                        )
                        .await?
                    }
                    _ => {
                        return Err(LamarrsServiceError::Service {
                            service: self.to_string(),
                        })
                    }
                }
            }
        }
    }

    /// Inserts or updates a Client into the current Service target list.
    /// Services hold their TargetClients in a HashMap using the Client UUID as Key.
    async fn upsert_target_client(
        &mut self,
        client_id_and_location: ClientIdAndLocation,
        client_sender: Sender<ExchangeMessage>,
    ) -> Result<(), LamarrsServiceError> {
        info!(
            ?client_id_and_location,
            "Processing adding Client to Service {}",
            self.to_string()
        );

        let target_map = self.get_target_client_map();

        match target_map.entry(client_id_and_location.id) {
            hash_map::Entry::Occupied(mut client_entry) => {
                info!(?client_id_and_location.id, "Found entry for");
                let client: &mut TargetClient = client_entry.get_mut();
                // If already subscribed, it uses the saved sender to notify the Client.
                Ok(client
                    .sender
                    .send(ExchangeMessage::Nack(NackResult::AlreadySubscribed))
                    .await?)
            }
            hash_map::Entry::Vacant(_) => {
                // This function inserts if it doesn't exist and updates if it does. Perfect for upserting.
                target_map.insert(
                    client_id_and_location.id,
                    TargetClient {
                        sender: client_sender.clone(),
                        location: client_id_and_location.location,
                    },
                );
                Ok(client_sender
                    .send(ExchangeMessage::Ack(AckResult::Success))
                    .await?) // If subscribed successfully, it uses the received sender to notify the Client.
            }
        }
    }

    /// Removes a Client from the Service target list.
    async fn remove_target_client(
        &mut self,
        client_id_and_location: ClientIdAndLocation,
        client_sender: Sender<ExchangeMessage>,
    ) -> Result<(), LamarrsServiceError> {
        info!(
            ?client_id_and_location,
            "Processing removing Client from Service {}",
            self.to_string()
        );

        let target_map = self.get_target_client_map();

        match target_map.entry(client_id_and_location.id) {
            hash_map::Entry::Occupied(client_entry) => {
                info!(?client_id_and_location.id, "Found entry for");
                client_entry.remove_entry();
                // If already subscribed, it uses the saved sender to notify the Client.
                Ok(client_sender
                    .send(ExchangeMessage::Ack(AckResult::Success))
                    .await?)
            }
            hash_map::Entry::Vacant(_) => {
                Ok(client_sender
                    .send(ExchangeMessage::Nack(NackResult::Failed))
                    .await?) // If it wasn't subscribed, returns Failed.
            }
        }
    }

    async fn write_to_target_clients(
        &mut self,
        message_for_subscribed_clients: Action,
        relative_location: Option<RelativeLocation>,
    ) -> Result<(), LamarrsServiceError> {
        info!(
            "Updating all the target Client from Service {}. Target location is: {:?}",
            self.to_string(),
            relative_location
        );

        if !self.action_is_allowed(&message_for_subscribed_clients) {
            return Err(LamarrsServiceError::NotAllowedMessageType {
                action_message: message_for_subscribed_clients.to_string(),
                service: self.to_string(),
            });
        }

        let target_senders_filtered_by_location = self
            .get_target_client_map()
            .into_iter()
            .filter_map(|(_, target_client)| {
                if relative_location.is_some() {
                    if target_client.location == relative_location {
                        Some(&target_client.sender)
                    } else {
                        None
                    }
                } else if target_client.location.is_none() {
                    Some(&target_client.sender)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for sender in target_senders_filtered_by_location {
            sender
                .send(ExchangeMessage::Update(Event::UpdateClient(
                    message_for_subscribed_clients.clone(),
                )))
                .await?
        }
        Ok(())
    }
}
