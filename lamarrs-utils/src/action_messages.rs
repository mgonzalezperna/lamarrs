use serde::{Deserialize, Serialize};
use strum::Display;

use crate::{AudioFile, ClientIdAndLocation, ColourRgb, Service, Subtitles};

/// These are the payloads the clients will be sending inside the Exchange Messages.
/// In the future, they may be also the payloads between services. Some feature gating
/// will be required for it.
#[derive(Deserialize, Serialize, PartialEq, Debug, Display)]
pub enum Event {
    Register(ClientIdAndLocation),
    SuscribeToService(Service, ClientIdAndLocation),
    UnsubscribeFromService(Service, ClientIdAndLocation),
    UpdateLocation(ClientIdAndLocation),
    UpdateClient(Action),
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
