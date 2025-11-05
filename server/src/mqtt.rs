use lamarrs_utils::orchestration_messages::OrchestrationMessage;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, Publish, QoS};
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::{debug, error, instrument};

use crate::services::InternalEventMessageServer;

pub struct MqttInterface {
    subtitles: Sender<InternalEventMessageServer>,
    colour: Sender<InternalEventMessageServer>,
    playback_audio: Sender<InternalEventMessageServer>,

    mqtt_sender: AsyncClient,
    mqtt_receiver: EventLoop,
}

impl MqttInterface {
    #[instrument(name = "MqttInterface::new", level = "INFO")]
    pub fn new(
        subtitles: Sender<InternalEventMessageServer>,
        colour: Sender<InternalEventMessageServer>,
        playback_audio: Sender<InternalEventMessageServer>,
    ) -> Self {
        let host = "192.168.178.70";
        let port: u16 = 1883;

        let mut mqttoptions = MqttOptions::new("lamarrs-server", host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));
        let (mqtt_sender, mqtt_receiver) = AsyncClient::new(mqttoptions, 10);

        Self {
            subtitles,
            colour,
            playback_audio,
            mqtt_sender,
            mqtt_receiver,
        }
    }

    #[instrument(name = "MqttInterface::send", skip(self), level = "INFO")]
    pub async fn send(&mut self, message: String) {
        // Basic implementation, in the future must have a good API to report Subscribers info, errors, etc.
        self.mqtt_sender
            .publish("lamarrs/orchestrator", QoS::AtLeastOnce, false, message)
            .await
            .unwrap();
    }

    #[instrument(name = "MqttInterface::run", skip(self), level = "INFO")]
    pub async fn run(&mut self) -> () {
        self.mqtt_sender
            .subscribe("lamarrs/orchestrator", QoS::AtMostOnce)
            .await
            .unwrap();
        loop {
            while let Ok(message) = self.mqtt_receiver.poll().await {
                match message {
                    Event::Incoming(Packet::Publish(packet)) => {
                        self.on_mqtt_published(packet).await;
                    }
                    _ => {} //error!("Unsupported message type received"),
                }
            }
        }
    }

    #[instrument(name = "MqttInterface::run", skip(self), level = "INFO")]
    pub async fn on_mqtt_published(&mut self, packet: Publish) {
        debug!(?packet.payload, "Payload:");
        match serde_json::from_slice(&packet.payload) {
            Ok::<OrchestrationMessage, serde_json::Error>(message) => match message {
                OrchestrationMessage::Request(action_message, relative_location) => {
                    match action_message {
                        lamarrs_utils::action_messages::Event::UpdateClient(
                            service_action,
                        ) => match &service_action {
                            lamarrs_utils::action_messages::Action::ShowNewSubtitles(_)=>{self.subtitles.send(InternalEventMessageServer::UpdateClients(service_action,relative_location,)).await;}
                            lamarrs_utils::action_messages::Action::ChangeColour(_)=>{self.colour.send(InternalEventMessageServer::UpdateClients(service_action,relative_location,)).await;}
                            lamarrs_utils::action_messages::Action::PlayAudio(_)=>{self.playback_audio.send(InternalEventMessageServer::UpdateClients(service_action,relative_location,)).await;}
                            _ => error!("Unsuported Action message request: {}", service_action)
                        },
                        _ => error!("Action Message not supported."),
                    }
                }
            },
            Err(msg) => {
                error!(?msg, "An error happened!")
            }
        };
    }
}
