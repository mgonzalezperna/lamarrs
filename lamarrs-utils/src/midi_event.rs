// Hack because MidiEvent doesn't implement Debug. That lib needs some love...

use std::path::Display;

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

impl std::fmt::Display for MidiEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = serde_json::to_value(self).unwrap();
        write!(f, "{}", value)
    }
}

impl TryFrom<oxisynth::MidiEvent> for MidiEvent {
    type Error = &'static str;

    fn try_from(value: oxisynth::MidiEvent) -> Result<Self, Self::Error> {
        match value {
            oxisynth::MidiEvent::NoteOn { channel, key, vel } => Ok(MidiEvent::NoteOn { channel, key, vel }),
            oxisynth::MidiEvent::NoteOff { channel, key } => Ok(MidiEvent::NoteOff { channel, key }),
            oxisynth::MidiEvent::ControlChange { channel, ctrl, value } => Ok(MidiEvent::ControlChange { channel, ctrl, value }),
            oxisynth::MidiEvent::AllNotesOff { channel } => Ok(MidiEvent::AllNotesOff { channel }),
            oxisynth::MidiEvent::AllSoundOff { channel } => Ok(MidiEvent::AllNotesOff { channel }),
            oxisynth::MidiEvent::PitchBend { channel, value } => Ok(MidiEvent::PitchBend { channel, value }),
            oxisynth::MidiEvent::ProgramChange { channel, program_id } => Ok(MidiEvent::ProgramChange { channel, program_id }),
            oxisynth::MidiEvent::ChannelPressure { channel, value } => Ok(MidiEvent::ChannelPressure { channel, value }),
            oxisynth::MidiEvent::PolyphonicKeyPressure { channel, key, value } => Ok(MidiEvent::PolyphonicKeyPressure { channel, key, value }),
            oxisynth::MidiEvent::SystemReset => Ok(MidiEvent::SystemReset),
        }
    }
}

impl TryFrom<MidiEvent> for oxisynth::MidiEvent {
    type Error = &'static str;

    fn try_from(value: MidiEvent) -> Result<Self, Self::Error> {
        match value {
            MidiEvent::NoteOn { channel, key, vel } => Ok(oxisynth::MidiEvent::NoteOn { channel, key, vel }),
            MidiEvent::NoteOff { channel, key } => Ok(oxisynth::MidiEvent::NoteOff { channel, key }),
            MidiEvent::ControlChange { channel, ctrl, value } => Ok(oxisynth::MidiEvent::ControlChange { channel, ctrl, value }),
            MidiEvent::AllNotesOff { channel } => Ok(oxisynth::MidiEvent::AllNotesOff { channel }),
            MidiEvent::AllSoundOff { channel } => Ok(oxisynth::MidiEvent::AllNotesOff { channel }),
            MidiEvent::PitchBend { channel, value } => Ok(oxisynth::MidiEvent::PitchBend { channel, value }),
            MidiEvent::ProgramChange { channel, program_id } => Ok(oxisynth::MidiEvent::ProgramChange { channel, program_id }),
            MidiEvent::ChannelPressure { channel, value } => Ok(oxisynth::MidiEvent::ChannelPressure { channel, value }),
            MidiEvent::PolyphonicKeyPressure { channel, key, value } => Ok(oxisynth::MidiEvent::PolyphonicKeyPressure { channel, key, value }),
            MidiEvent::SystemReset => Ok(oxisynth::MidiEvent::SystemReset),
        }
    }
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
