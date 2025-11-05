use serde::{Deserialize, Serialize};

use crate::{action_messages::Action, RelativeLocation};

/// Wrapper for any message traveling between the Orchestrator and the Server
///  * Request: Orchestrator > Server. Request sent by the Orchestrator to the server to perform an action.
#[derive(Deserialize, Serialize, PartialEq, Debug)]
pub enum OrchestrationMessage {
    Request(Action, Option<RelativeLocation>),
}
