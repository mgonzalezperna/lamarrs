use serde::{Deserialize, Serialize};
use strum::Display;

use crate::{action_messages::Event, ErrorDescription};

/// Wrapper for the messages traveling between the Clients and the Server
///
/// These are:
///  * Ack: Server > Client. Confirmation of requested action and results.
///  * Nack: Server > Client. Rejection of request and its reason.
///  * Request: Client > Server. Request sent by the client to the server to perform an action.
///  * NewScene: Server > Client. Update sent one of the service the client is subscribed to.
///  * Error: Server > Client. Jocker type message for yet-unmapped error cases.
///  * NextScene: Client > Server. Move to the next orchestrated scene. Sent by a client operated by a Scene commander -button, timer, etc.
///  * RetriggerScene: Client > Server. Same as NextScene but retriggers the same scene. Sent by a client operated by a Scene commander -button, timer, etc.
///  * Heartbeat & HeartbeatAck: Client > Server.
#[derive(Deserialize, Display, Serialize, PartialEq, Debug, Clone)]
pub enum ExchangeMessage {
    Ack(AckResult),
    Nack(NackResult),
    Request(Event),
    Scene(Event),
    Error(ErrorDescription),
    NextScene,
    RetriggerScene,
    Heartbeat,
    HeartbeatAck,
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
