// Collection of messages types to be send via websockets.

use serde::{Deserialize, Serialize};
use crate::enums::{RelativeLocation, Service};
use arrayvec::ArrayString;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suscribe{
    pub uuid: Uuid,
    pub service: Service,
    pub location: RelativeLocation
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subtitle{
    pub subtitle: ArrayString<35>,
}