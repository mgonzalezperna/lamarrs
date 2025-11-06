use serde::{Deserialize, Serialize};
use strum::Display;

use crate::{action_messages::Event, ErrorDescription};

/// Wrapper for the messages traveling between the Clients and the Server
///
/// There are 5 types:
///  * Ack: Server > Client. Confirmation of requested action and results.
///  * Nack: Server > Client. Rejection of request and its reason.
///  * Request: Client > Server. Request sent by the client to the server to perform an action.
///  * Update: Server > Client. Update sent one of the service the client is subscribed to.
///  * Error: Server > Client. Jocker type message for yet-unmapped error cases.
#[derive(Deserialize, Display, Serialize, PartialEq, Debug)]
pub enum ExchangeMessage {
    Ack(AckResult),
    Nack(NackResult),
    Request(Event),
    Update(Event),
    Error(ErrorDescription),
}

/// Results on the latest request sent by client if succeeds.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum AckResult {
    Success,
    UpdatedSubscription,
    UpdatedLocation,
}

/// Results on the latest request sent by client if fails.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum NackResult {
    AlreadySubscribed,
    NotSubscribed,
    Failed,
}
