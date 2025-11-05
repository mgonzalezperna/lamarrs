#![no_std]
#![no_main]

pub mod action_messages;
pub mod exchange_messages;
pub mod orchestration_messages;
// pub mod midi_event;  I don´t know if this lib is no_std and I don´t need MIDI it right now.

use heapless::String;
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use strum::{Display, EnumIter};
use uuid::Uuid;

/// Relative Location of the Client. Useful for certain special effects involving sound and colours.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, EnumIter, Display)]
pub enum RelativeLocation {
    Left,
    Center,
    Right,
}

/// Client data, relevant to identify itself and to update the location if needed.
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct ClientIdAndLocation {
    pub id: Uuid,
    pub location: Option<RelativeLocation>,
}

impl ClientIdAndLocation {
    pub fn new(id: Uuid, location: Option<RelativeLocation>) -> Self {
        Self {id, location}
    }
}

// Put this under a feature?
// impl fmt::Display for ClientIdAndLocation {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         match (self.uuid, self.location.clone()) {
//             (None, _) => write!(f, "(Unregistered Subscriber, {})", self.addr),
//             (Some(uuid), None) => {
//                 write!(f, "(id:{}, location: Unknown, address:{})", uuid, self.addr)
//             }
//             (Some(uuid), Some(location)) => write!(
//                 f,
//                 "(id:{}, location:{}, address:{})",
//                 uuid, location, self.addr
//             ),
//         }
//     }
// }

/// List of all the currently supported services.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, EnumIter, Display)]
pub enum Service {
    Subtitle,
    Colour,
    AudioPlayer,
}

/// Payload for colour change requests. TODO: allow other formats, or impl `to_hex()`, something like that.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ColourRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColourRgb {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self {r: red, g: green, b: blue}
    }
}


/* ################################################################################################*/

/// Payload for subtitle change requests. The String can have up to 50 chars per line, following the
/// avg of the current streaming services subtitle conventions.
#[derive(Clone, Debug, PartialEq)]
pub struct Subtitles {
    pub subtitles: String<50>,
}

// Manual Serialize / Deserialize Subtitle, as Derive can´t do it.
impl Serialize for Subtitles {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.subtitles.as_str())
    }
}

impl<'de> Deserialize<'de> for Subtitles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize into a regular String first
        let subtitle = String::<50>::try_from(<&str>::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)?;

        Ok(Subtitles {
            subtitles: subtitle,
        })
    }
}

/* ################################################################################################*/

/// Payload for Audio Playback requests. The String can have up to 50 chars per line.
/// More, better validations can come in the future, as format to play, or other params.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioFile {
    pub file_name: String<50>,
    pub file_extension: String<4>,
}

// Manual Serialize / Deserialize AudioFile, as Derive can´t do it.
impl Serialize for AudioFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serializing a struct with 2 fields
        let mut state = serializer.serialize_struct("AudioFile", 2)?;
        state.serialize_field("file_name", self.file_name.as_str())?;
        state.serialize_field("file_extension", self.file_extension.as_str())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AudioFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Define a helper struct to receive the normal Strings from serde
        #[derive(Deserialize)]
        struct AudioFileHelper<'a> {
            file_name: &'a str,
            file_extension: &'a str,
        }

        let helper = AudioFileHelper::deserialize(deserializer)?;
        Ok(AudioFile {
            file_name: String::<50>::try_from(helper.file_name)
                .map_err(serde::de::Error::custom)?,
            file_extension: String::<4>::try_from(helper.file_extension)
                .map_err(serde::de::Error::custom)?,
        })
    }
}

impl AudioFile {
    pub fn file_name_with_extension(&self) -> String<55> {
        let mut file_name_with_extension: String<55> = String::new();
        use core::fmt::Write;
        let _ = write!(file_name_with_extension, "{}.{}", self.file_name, self.file_extension);
        file_name_with_extension
    }
}

/* ################################################################################################*/

/// Payload for error message descriptions. The 100 chars limit is completely arbitrary.
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorDescription {
    pub error_descr: String<100>,
}

// Manual Serialize / Deserialize Subtitle, as Derive can´t do it.
impl Serialize for ErrorDescription {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.error_descr.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorDescription {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize into a regular String first
        let error_descr = String::<100>::try_from(<&str>::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)?;

        Ok(ErrorDescription { error_descr })
    }
}
