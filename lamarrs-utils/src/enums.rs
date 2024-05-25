use serde::{Deserialize, Serialize};
use crate::messages::*;

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub enum GatewayMessage{
    SubscribeResult(SubscribeResult),
    Subtitle(Subtitle),
    CloseConnection(CloseConnectionReason)
}

impl std::fmt::Display for GatewayMessage{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::SubscribeResult(inner) => serde_json::to_value(inner).unwrap(),
            Self::Subtitle(inner) => serde_json::to_value(inner).unwrap(),
            Self::CloseConnection(inner)=> serde_json::to_value(inner).unwrap(),
        };
        write!(f, "{}", value)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SubscriberMessage{
    // A Client wants to subscribe to the service.
    Subscribe(Subscribe),
    // A Client wants to update its location.
    Location(Location),
    CloseConnection(CloseConnectionReason),
}

impl std::fmt::Display for SubscriberMessage{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Subscribe(inner) => serde_json::to_value(inner).unwrap(),
            Self::Location(inner) => serde_json::to_value(inner).unwrap(),
            Self::CloseConnection(inner)=> serde_json::to_value(inner).unwrap(),
        };
        write!(f, "{}", value)
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