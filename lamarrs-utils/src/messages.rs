use serde::{Deserialize, Serialize};
use crate::enums::RelativeLocation;
use arrayvec::ArrayString;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResult {
    pub result: ArrayString<20>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscribe {
    pub service: ArrayString<20>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub location: RelativeLocation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subtitle{
    pub subtitle: ArrayString<35>,
}

