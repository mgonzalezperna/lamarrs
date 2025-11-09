pub mod service;

use lamarrs_utils::{
    action_messages::{Action, Event},
    exchange_messages::{AckResult, ExchangeMessage, NackResult},
    ClientIdAndLocation, RelativeLocation,
};
use tokio::sync::mpsc::{self, Sender};
use tracing::{error, info, instrument};
use uuid::Uuid;

use std::{
    collections::{
        hash_map::{self, Entry},
        HashMap,
    },
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
    #[error("Error sending an InternalEventMessageServer")]
    SendInternalEventMessage(#[from] mpsc::error::SendError<InternalEventMessageServer>),
    #[error("The message type {} requested to send to the Target Clients is not allowed for the service {}.", action_message, service)]
    NotAllowedMessageType {
        action_message: String,
        service: String,
    },
    #[error("Client was not found in the list of {} subscribers.", service)]
    ClientNotFound { service: String },
    #[error("Client is already subscribed to {}.", service)]
    ClientAlreadySubscribed { service: String },
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
    RemoveTargetClient(ClientIdAndLocation),
    UpdateClientData(ClientIdAndLocation, Sender<ExchangeMessage>),
    PerformAction(Action, Option<RelativeLocation>),
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
    #[instrument(name = "service::run", skip(self), fields(service=self.to_string()), level = "INFO", ret, err)]
    async fn run(&mut self) -> Result<(), LamarrsServiceError> {
        loop {
            while let Some(message) = self.receive_message().await {
                let results = match message {
                    InternalEventMessageServer::AddTargetClient(
                        client_id_and_location,
                        client_sender,
                    ) => {
                        self.insert_target_client(client_id_and_location, client_sender)
                            .await
                    }
                    InternalEventMessageServer::UpdateClientData(
                        client_id_and_location,
                        client_sender,
                    ) => {
                        self.update_target_client(client_id_and_location, client_sender)
                            .await
                    }
                    InternalEventMessageServer::RemoveTargetClient(client_id_and_location) => {
                        self.remove_target_client(client_id_and_location).await
                    }
                    InternalEventMessageServer::PerformAction(
                        message_for_subscribed_clients,
                        relative_location,
                    ) => {
                        self.write_to_target_clients(
                            message_for_subscribed_clients,
                            relative_location,
                        )
                        .await
                    }
                    _ => {
                        return Err(LamarrsServiceError::Service {
                            service: self.to_string(),
                        })
                    }
                };
                if let Err(service_error) = results {
                    error!("{:?}", service_error)
                };
            }
        }
    }

    /// Updates a Client into the current Service target list if already exists.
    /// Services hold their TargetClients in a HashMap using the Client UUID as Key.
    async fn update_target_client(
        &mut self,
        client_id_and_location: ClientIdAndLocation,
        new_client_sender: Sender<ExchangeMessage>,
    ) -> Result<(), LamarrsServiceError> {
        info!(
            ?client_id_and_location,
            "Processing adding Client to Service {}",
            self.to_string()
        );

        let target_map = self.get_target_client_map();

        if let Entry::Occupied(mut client_entry) = target_map.entry(client_id_and_location.uuid) {
            info!(?client_id_and_location.uuid, "Found entry for");
            let client: &mut TargetClient = client_entry.get_mut();
            client.sender = new_client_sender;
            // If already subscribed, it uses the saved sender to notify the Client.
            Ok(client
                .sender
                .send(ExchangeMessage::Ack(AckResult::UpdatedSubscription))
                .await?)
        } else {
            Err(LamarrsServiceError::ClientNotFound {
                service: self.to_string(),
            })
        }
    }

    /// Inserts a Client into the current Service target list.
    /// Services hold their TargetClients in a HashMap using the Client UUID as Key.
    async fn insert_target_client(
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

        match target_map.entry(client_id_and_location.uuid) {
            hash_map::Entry::Occupied(mut client_entry) => {
                info!(?client_id_and_location.uuid, "Found entry for");
                let client: &mut TargetClient = client_entry.get_mut();
                // If already subscribed, it uses the saved sender to notify the Client.
                client
                    .sender
                    .send(ExchangeMessage::Nack(NackResult::AlreadySubscribed))
                    .await?;
                Err(LamarrsServiceError::ClientAlreadySubscribed {
                    service: self.to_string(),
                })
            }
            hash_map::Entry::Vacant(_) => {
                // This function inserts if it doesn't exist and updates if it does. Perfect for upserting.
                target_map.insert(
                    client_id_and_location.uuid,
                    TargetClient {
                        sender: client_sender.clone(),
                        location: client_id_and_location.location,
                    },
                );
                client_sender
                    .send(ExchangeMessage::Ack(AckResult::Success))
                    .await?; // If subscribed successfully, it uses the received sender to notify the Client.
                Ok(())
            }
        }
    }

    /// Removes a Client from the Service target list.
    async fn remove_target_client(
        &mut self,
        client_id_and_location: ClientIdAndLocation,
    ) -> Result<(), LamarrsServiceError> {
        info!(
            ?client_id_and_location,
            "Processing removing Client from Service {}",
            self.to_string()
        );

        let target_map = self.get_target_client_map();

        match target_map.entry(client_id_and_location.uuid) {
            hash_map::Entry::Occupied(client_entry) => {
                info!(?client_id_and_location.uuid, "Found entry for");
                client_entry.remove_entry();
                Ok(())
            }
            hash_map::Entry::Vacant(_) => Err(LamarrsServiceError::ClientNotFound {
                service: self.to_string(),
            }),
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
                .send(ExchangeMessage::Scene(Event::PerformAction(
                    message_for_subscribed_clients.clone(),
                )))
                .await?
        }
        Ok(())
    }
}
