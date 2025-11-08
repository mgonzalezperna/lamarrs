use core::fmt::Write;
use serde::{Deserialize, Serialize};
use strum::Display;
use heapless::String;

use crate::{AudioFile, ClientIdAndLocation, ColourRgb, Service, Subtitles};

/// These are the payloads the clients will be sending inside the Exchange Messages.
/// In the future, they may be also the payloads between services. Some feature gating
/// will be required for it.
#[derive(Deserialize, Serialize, PartialEq, Debug, Display, Clone)]
pub enum Event {
    Register(ClientIdAndLocation),
    SuscribeToService(Service, ClientIdAndLocation),
    UnsubscribeFromService(Service, ClientIdAndLocation),
    UpdateLocation(ClientIdAndLocation),
    PerformAction(Action),
}

/// Internal message types to be transmited between actors inside Lamarrs.
/// These are also the payloads the clients will be sending inside the Exchange Messages.
#[derive(Deserialize, Serialize, PartialEq, Debug, Display, Clone)]
pub enum Action {
    ShowNewSubtitles(Subtitles),
    ChangeColour(ColourRgb),
    PlayAudio(AudioFile),
    //MIDIEvent(MidiEvent), We are dropping MIDI support by now, since this utils must be no_std friendly.
}

impl Action {
    pub fn as_str<'a> (&self, write_buffer: &'a mut heapless::String<128>) -> &'a str {
        write_buffer.clear();
        match self {
            Action::ShowNewSubtitles(subtitles) => {
                let _ = write!(write_buffer, "Subs {:?}", subtitles.subtitles);
            }
            Action::ChangeColour(colour_rgb) => {
                let _ = write!(
                    write_buffer,
                    "Colour ({:?}/{:?}/{:?})",
                    colour_rgb.r, colour_rgb.g, colour_rgb.b
                );
            }
            Action::PlayAudio(audio_file) => {
                let _ = write!(write_buffer, "Audio {:?}", audio_file.file_name);
            }
        }
        write_buffer.as_str()
    }
}