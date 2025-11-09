use std::{collections::VecDeque, path::PathBuf};

use crate::{
    sequencer::sequence_parser::{Sequence, SequenceStep},
    services::{InternalEventMessageServer, LamarrsServiceError},
};
use async_time_mock_tokio::MockableClock;
use lamarrs_utils::{action_messages::Action, exchange_messages::ExchangeMessage};
use tokio::{
    fs,
    sync::mpsc::{channel, Receiver, Sender},
};
use tracing::{debug, error, info, instrument};

mod sequence_parser;

pub struct Sequencer {
    subtitles_service: Sender<InternalEventMessageServer>,
    colour_service: Sender<InternalEventMessageServer>,
    playback_service: Sender<InternalEventMessageServer>,

    pub sender: Sender<ExchangeMessage>,
    inbox: Receiver<ExchangeMessage>,
    pub sequence_path: PathBuf,
    last_sequence_step_played: Option<SequenceStep>,
    clock: MockableClock,
}

impl Sequencer {
    pub fn new(
        subtitles_service: Sender<InternalEventMessageServer>,
        colour_service: Sender<InternalEventMessageServer>,
        playback_service: Sender<InternalEventMessageServer>,
        sequence_path: PathBuf,
    ) -> Self {
        let (sender, inbox) = channel(32);
        Self {
            subtitles_service,
            colour_service,
            playback_service,
            sender,
            inbox,
            sequence_path,
            last_sequence_step_played: None,
            clock: MockableClock::Real,
        }
    }

    #[instrument(
        name = "service::load_sequence",
        skip(self),
        fields(service = "Sequencer"),
        level = "INFO",
        ret,
        err
    )]
    pub async fn load_show_sequence(&mut self) -> Result<Sequence, LamarrsServiceError> {
        // Load and parse YAML from file.
        let yaml = fs::read_to_string(&self.sequence_path).await.map_err(|_| {
            LamarrsServiceError::Service {
                service: "Sequencer".into(),
            }
        })?;
        debug!("Loaded sequence from file");
        match serde_yaml::from_str(&yaml) {
            Ok(sequence) => {
                debug!("Sequence results {:?}", sequence);
                Ok(sequence)
            }
            Err(err) => {
                error!("Failed to serialise file into Sequence: {}", err);
                return Err(LamarrsServiceError::Service {
                    service: "Sequencer".into(),
                });
            }
        }
    }

    #[instrument(
        name = "service::run",
        skip(self),
        fields(service = "Sequencer"),
        level = "INFO",
        ret,
        err
    )]
    pub async fn run(&mut self) -> Result<(), LamarrsServiceError> {
        let mut sequence = self.load_show_sequence().await?.sequence;
        loop {
            while let Some(message) = self.inbox.recv().await {
                match message {
                    ExchangeMessage::NextScene => {
                        if let Some(mut sequence_step) = sequence.pop_front() {
                            // We save the step sequence that was manually triggered, as the most likeable case is
                            // that the artist needs to re do the sequence from the beggining.
                            self.last_sequence_step_played = Some(sequence_step.clone());
                            // If the sequence step has a pre programmed duration, we automatise the steps.
                            while let Some(timeout) = sequence_step.duration {
                                self.dispatch_action_to_perform(&sequence_step).await?;
                                // We wait before executing the next step.
                                info!("Next step to be executed in {} seconds", timeout.as_secs());
                                tokio::time::sleep(timeout).await;
                                if let Some(next_sequence_step) = sequence.pop_front() {
                                    sequence_step = next_sequence_step;
                                } else {
                                    info!("Sequence finished! Restart the show or restart the service with a new Sequence.")
                                }
                            }
                            // Process the step that has None as `duration`.
                            self.dispatch_action_to_perform(&sequence_step).await?
                        } else {
                            info!("Sequence finished! Restart the show or restart the service with a new Sequence.")
                        }
                    }
                    ExchangeMessage::RetriggerScene => {
                        if let Some(sequence_step) = &self.last_sequence_step_played {
                            self.dispatch_action_to_perform(sequence_step).await?
                        } else {
                            error!("There is no previous sequence step played yet!");
                        }
                    }
                    _ => error!("Invalid ExchangeMessage received. Discarding message."),
                }
            }
        }
    }

    async fn dispatch_action_to_perform(
        &self,
        sequence_step: &SequenceStep,
    ) -> Result<(), LamarrsServiceError> {
        info!("Executing step named {}.", sequence_step.name);
        match sequence_step.action {
            Action::ShowNewSubtitles(_) => {
                self.subtitles_service
                    .send(InternalEventMessageServer::PerformAction(
                        sequence_step.action.clone(),
                        sequence_step.target_location.clone(),
                    ))
                    .await?
            }
            Action::ChangeColour(_) => {
                self.colour_service
                    .send(InternalEventMessageServer::PerformAction(
                        sequence_step.action.clone(),
                        sequence_step.target_location.clone(),
                    ))
                    .await?
            }
            Action::PlayAudio(_) => {
                self.playback_service
                    .send(InternalEventMessageServer::PerformAction(
                        sequence_step.action.clone(),
                        sequence_step.target_location.clone(),
                    ))
                    .await?
            }
        };
        Ok(())
    }
}
