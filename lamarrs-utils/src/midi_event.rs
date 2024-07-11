// Hack because MidiEvent doesn't implement Debug. That lib needs some love...

use oxisynth::{SoundFont, TypedIndex};
use serde::{Deserialize, Serialize};

pub type U7 = u8;
pub type U14 = u16;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum MidiEvent {
    /// Send a noteon message.
    NoteOn {
        channel: u8,
        key: U7,
        vel: U7,
    },
    /// Send a noteoff message.
    NoteOff {
        channel: u8,
        key: U7,
    },
    /// Send a control change message.
    ControlChange {
        channel: u8,
        ctrl: U7,
        value: U7,
    },
    AllNotesOff {
        channel: u8,
    },
    AllSoundOff {
        channel: u8,
    },
    /// Send a pitch bend message.
    PitchBend {
        channel: u8,
        value: U14,
    },
    /// Send a program change message.
    ProgramChange {
        channel: u8,
        program_id: U7,
    },
    /// Set channel pressure
    ChannelPressure {
        channel: u8,
        value: U7,
    },
    /// Set key pressure (aftertouch)
    PolyphonicKeyPressure {
        channel: u8,
        key: U7,
        value: U7,
    },
    /// Send a reset.
    ///
    /// A reset turns all the notes off and resets the controller values.
    ///
    /// Purpose:
    /// Respond to the MIDI command 'system reset' (0xFF, big red 'panic' button)
    SystemReset,
}

impl MidiEvent {
    pub fn check(self) -> Result<Self, OxiError> {
        match &self {
            MidiEvent::NoteOn { key, vel, .. } => {
                range_check(0..=127, key, OxiError::KeyOutOfRange)?;
                range_check(0..=127, vel, OxiError::VelocityOutOfRange)?;
            }
            MidiEvent::NoteOff { key, .. } => {
                range_check(0..=127, key, OxiError::KeyOutOfRange)?;
            }
            MidiEvent::ControlChange { ctrl, value, .. } => {
                range_check(0..=127, ctrl, OxiError::CtrlOutOfRange)?;
                range_check(0..=127, value, OxiError::CCValueOutOfRange)?;
            }
            MidiEvent::AllNotesOff { .. } => {}
            MidiEvent::AllSoundOff { .. } => {}
            MidiEvent::PitchBend { value, .. } => {
                range_check(0..=16383, value, OxiError::PithBendOutOfRange)?;
            }
            MidiEvent::ProgramChange { program_id, .. } => {
                range_check(0..=127, program_id, OxiError::ProgramOutOfRange)?;
            }
            MidiEvent::ChannelPressure { value, .. } => {
                range_check(0..=127, value, OxiError::ChannelPressureOutOfRange)?;
            }
            MidiEvent::PolyphonicKeyPressure { key, value, .. } => {
                range_check(0..=127, key, OxiError::KeyOutOfRange)?;
                range_check(0..=127, value, OxiError::KeyPressureOutOfRange)?;
            }
            MidiEvent::SystemReset => {}
        };

        Ok(self)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum OxiError {
    #[error("Key out of range (0-127)")]
    KeyOutOfRange,
    #[error("Velocity out of range (0-127)")]
    VelocityOutOfRange,
    #[error("Channel out of range")]
    ChannelOutOfRange,
    #[error("Ctrl out of range (0-127)")]
    CtrlOutOfRange,
    #[error("CC Value out of range (0-127)")]
    CCValueOutOfRange,
    #[error("Program out of range")]
    ProgramOutOfRange,
    #[error("Key pressure out of range (0-127)")]
    KeyPressureOutOfRange,
    #[error("Channel pressure out of range (0-127)")]
    ChannelPressureOutOfRange,
    #[error("PithBend out of range")]
    PithBendOutOfRange,
    #[error("Channel has no preset")]
    ChannelHasNoPreset,
    #[error(
        "There is no preset with bank number {bank_id} and preset number {preset_id} in SoundFont {sfont_id}"
    )]
    PresetNotFound {
        bank_id: u32,
        preset_id: u8,
        sfont_id: TypedIndex<SoundFont>,
    },
}

fn range_check<E, T: PartialOrd, C: std::ops::RangeBounds<T>>(
    range: C,
    value: &T,
    error: E,
) -> Result<(), E> {
    if range.contains(value) {
        Ok(())
    } else {
        Err(error)
    }
}
