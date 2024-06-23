use lamarrs_utils::enums::{GatewayMessage, OrchestratorMessage};
use lamarrs_utils::error::InternalError;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, Publish, QoS};
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::{task, time};
use tracing::{debug, error, instrument};

use crate::services::text_streamers::{ColorMessage, SubtitleMessage};

pub struct MqttInterface {
    pub sender: Sender<GatewayMessage>,
    receiver: Receiver<GatewayMessage>,
    subtitles: Sender<SubtitleMessage>,
    color: Sender<ColorMessage>,

    mqtt_sender: AsyncClient,
    mqtt_receiver: EventLoop,
}

impl MqttInterface {
    #[instrument(name = "MqttInterface::new", level = "INFO")]
    pub fn new(subtitles: Sender<SubtitleMessage>, color: Sender<ColorMessage>) -> Self {
        let (sender, receiver) = channel(32);
        let host = "localhost";
        let port: u16 = 1883;

        let mut mqttoptions = MqttOptions::new("lamarrs-server", host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));
        let (mut mqtt_sender, mut mqtt_receiver) = AsyncClient::new(mqttoptions, 10);

        Self {
            sender,
            receiver,
            subtitles,
            color,
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

    #[instrument(name = "MqttInterface::run", skip(self), level = "INFO", ret, err)]
    pub async fn run(&mut self) -> Result<(), InternalError> {
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
            Ok::<OrchestratorMessage, serde_json::Error>(message) => match message {
                OrchestratorMessage::SendSubtitle(subtitles_with_target) => {
                    self.subtitles
                        .send(SubtitleMessage::SendSubtitle(subtitles_with_target))
                        .await;
                }
                OrchestratorMessage::SendColor(color_with_target) => {
                    self.color
                        .send(ColorMessage::SendColor(color_with_target))
                        .await;
                }
                OrchestratorMessage::Error(_) => todo!(),
            },
            Err(msg) => {
                error!(?msg, "An error happened!")
            }
        };
    }
}
