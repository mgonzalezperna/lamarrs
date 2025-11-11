use lamarrs_utils::action_messages::Action;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use uuid::Uuid;

use std::{collections::HashMap, fmt};

use crate::services::{InternalEventMessageServer, LamarrsService, TargetClient};

#[derive(Debug)]
pub struct SubtitleService {
    targets: HashMap<Uuid, TargetClient>,
    pub sender: Sender<InternalEventMessageServer>,
    receiver: Receiver<InternalEventMessageServer>,
}

impl SubtitleService {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            targets: HashMap::new(),
            sender,
            receiver,
        }
    }
}

// Implement `Display` for `SubtitleService`.
impl fmt::Display for SubtitleService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", "SubtitlesService")
    }
}

// Implement `LamarrsService` for `SubtitleService`.
impl LamarrsService for SubtitleService {
    fn action_is_allowed(&self, message: &Action) -> bool {
        matches!(message, Action::ShowNewSubtitles(_))
    }
    fn get_target_client_map(&mut self) -> &mut HashMap<Uuid, TargetClient> {
        &mut self.targets
    }
    async fn receive_message(&mut self) -> Option<InternalEventMessageServer> {
        self.receiver.recv().await
    }
}

#[derive(Debug)]
pub struct ColourService {
    targets: HashMap<Uuid, TargetClient>,
    pub sender: Sender<InternalEventMessageServer>,
    receiver: Receiver<InternalEventMessageServer>,
}

impl ColourService {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            targets: HashMap::new(),
            sender,
            receiver,
        }
    }
}

// Implement `Display` for `ColourService`.
impl fmt::Display for ColourService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", "ColourService")
    }
}

// Implement `LamarrsService` for `ColoursService`.
impl LamarrsService for ColourService {
    fn action_is_allowed(&self, message: &Action) -> bool {
        matches!(message, Action::ChangeColour(_))
    }
    fn get_target_client_map(&mut self) -> &mut HashMap<Uuid, TargetClient> {
        &mut self.targets
    }

    async fn receive_message(&mut self) -> Option<InternalEventMessageServer> {
        self.receiver.recv().await
    }
}

#[derive(Debug)]
pub struct PlaybackService {
    targets: HashMap<Uuid, TargetClient>,
    pub sender: Sender<InternalEventMessageServer>,
    receiver: Receiver<InternalEventMessageServer>,
}

impl PlaybackService {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            targets: HashMap::new(),
            sender,
            receiver,
        }
    }
}

// Implement `Display` for `PlaybackService`.
impl fmt::Display for PlaybackService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", "PlaybackService")
    }
}

// Implement `LamarrsService` for `PlaybackService`.
impl LamarrsService for PlaybackService {
    fn action_is_allowed(&self, message: &Action) -> bool {
        matches!(message, Action::PlayAudio(_))
    }
    fn get_target_client_map(&mut self) -> &mut HashMap<Uuid, TargetClient> {
        &mut self.targets
    }

    async fn receive_message(&mut self) -> Option<InternalEventMessageServer> {
        self.receiver.recv().await
    }
}


#[derive(Debug)]
pub struct MidiService {
    targets: HashMap<Uuid, TargetClient>,
    pub sender: Sender<InternalEventMessageServer>,
    receiver: Receiver<InternalEventMessageServer>,
}

impl MidiService {
    pub fn new() -> Self {
        let (sender, receiver) = channel(32);
        Self {
            targets: HashMap::new(),
            sender,
            receiver,
        }
    }
}

// Implement `Display` for `MidiService`.
impl fmt::Display for MidiService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", "MidiService")
    }
}

// Implement `LamarrsService` for `MidiService`.
impl LamarrsService for MidiService {
    fn action_is_allowed(&self, message: &Action) -> bool {
        matches!(message, Action::Midi(_))
    }
    fn get_target_client_map(&mut self) -> &mut HashMap<Uuid, TargetClient> {
        &mut self.targets
    }

    async fn receive_message(&mut self) -> Option<InternalEventMessageServer> {
        self.receiver.recv().await
    }
}
