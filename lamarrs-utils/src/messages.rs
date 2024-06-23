// Collection of messages types to be send via websockets.

use std::{fmt, path::Display, str::FromStr, string::ParseError};

use crate::enums::{Color, RelativeLocation, Service};
use arrayvec::ArrayString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Suscribe {
    pub uuid: Uuid,
    pub service: Service,
    pub location: RelativeLocation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Subtitle {
    pub subtitle: ArrayString<35>,
}

impl FromStr for Subtitle {
    type Err = ParseError;

    fn from_str(value: &str) -> std::result::Result<Self, std::convert::Infallible> {
        let mut new_subtitles = Self {
            subtitle: ArrayString::<35>::new(),
        };
        new_subtitles.subtitle.push_str(value);
        Ok(new_subtitles)
    }
}

impl fmt::Display for Subtitle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.subtitle)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SendSubtitle {
    pub subtitle: Subtitle,
    pub target_location: RelativeLocation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SendColor {
    pub color: Color,
    pub target_location: RelativeLocation,
}
