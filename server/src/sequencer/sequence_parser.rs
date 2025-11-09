use std::{collections::VecDeque, time::Duration};

use lamarrs_utils::{action_messages::Action, RelativeLocation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sequence {
    pub version: u8,
    pub sequence: VecDeque<SequenceStep>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SequenceStep {
    pub name: String,
    pub action: Action,
    pub target_location: Option<RelativeLocation>,
    #[serde(deserialize_with = "humantime_serde::deserialize")]
    pub duration: Option<Duration>,
}
