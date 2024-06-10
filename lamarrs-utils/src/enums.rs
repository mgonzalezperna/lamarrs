use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::messages::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageError{
    #[error("Error guessing the Message type")]
    GuessingError(String),
    #[error("Failed to find Message Type: {0}")]
    SerializationError(#[from] serde_json::Error),
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub enum GatewayMessage{
    RegisterResult(RegisterResult),
    Suscribe(Suscribe),
    SubscribeResult(SubscribeResult),
    Subtitle(Subtitle),
    Color(Color),
    Error(GatewayError)
}

impl std::fmt::Display for GatewayMessage{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::RegisterResult(inner) => serde_json::to_value(inner).unwrap(),
            Self::Suscribe(inner) => serde_json::to_value(inner).unwrap(),
            Self::SubscribeResult(inner) => serde_json::to_value(inner).unwrap(),
            Self::Subtitle(inner) => serde_json::to_value(inner).unwrap(),
            Self::Color(inner) => serde_json::to_value(inner).unwrap(),
            Self::Error(inner) => serde_json::to_value(inner).unwrap(),
        };
        write!(f, "{}", value)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub enum GatewayError{
    UnregisteredSubscriber,
}

impl std::fmt::Display for GatewayError{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::UnregisteredSubscriber => "UnregisteredSubscriber"
        };
        write!(f, "{}", value)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SubscriberMessage{
    // The first message after connecting must be a Register, with its own UUID.
    Register((Uuid, RelativeLocation)),
    // A Client wants to subscribe to the service.
    Subscribe(Service),
    // A Client wants to update its location.
    CloseConnection(CloseConnectionReason),
}

impl std::fmt::Display for SubscriberMessage{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Register(inner) => serde_json::to_value(inner).unwrap(),
            Self::Subscribe(inner) => serde_json::to_value(inner).unwrap(),
            Self::CloseConnection(inner)=> serde_json::to_value(inner).unwrap(),
        };
        write!(f, "{}", value)
    }
}

impl SubscriberMessage {
    pub fn deserialize(data: String) -> Option<Self> {
        serde_json::from_str(&data).ok()
    } 
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum RelativeLocation{
    Left,
    Center,
    Right
}

impl std::fmt::Display for RelativeLocation{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Left=> "Left",
            Self::Center=> "Center",
            Self::Right=> "Right",
        };
        write!(f, "{}", value)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SubscribeResult {
    Success,
    AlreadySubscribed,
    NotSubscribed,
    UpdatedSubscription,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum RegisterResult{
    Success,
    AlreadyRegistered,
    Failed,
}


#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum CloseConnectionReason{
    SubscriberRequest,
    GatewatRequest,
    Unexpected
}

impl std::fmt::Display for CloseConnectionReason{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::SubscriberRequest=> "Subscriber Request",
            Self::GatewatRequest=> "Gatewat Request",
            Self::Unexpected=> "Unexpected",
        };
        write!(f, "{}", value)
    }
}