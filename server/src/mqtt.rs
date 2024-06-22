use lamarrs_utils::enums::{GatewayMessage, OrchestratorMessage};
use lamarrs_utils::error::InternalError;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, Publish, QoS};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::{task, time};
use std::time::Duration;
use std::error::Error;

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
    pub fn new(subtitles: Sender<SubtitleMessage>, color: Sender<ColorMessage>) -> Self {
        let (sender, receiver) = channel(32);

        let mut mqttoptions = MqttOptions::new("rumqtt-async", "test.mosquitto.org", 1883);
        mqttoptions.set_keep_alive(Duration::from_secs(5));
        let (mqtt_sender, mqtt_receiver) = AsyncClient::new(mqttoptions, 10);

        Self {
            sender,
            receiver,
            subtitles,
            color,
            mqtt_sender,
            mqtt_receiver
        }
    }

    pub async fn send(&mut self, message: String) {
        // Basic implementation, in the future must have a good API to report Subscribers info, errors, etc.
        self.mqtt_sender.publish("hello/rumqtt", QoS::AtLeastOnce, false, message).await.unwrap();
    }

    pub async fn run(&mut self) -> Result<(), InternalError> {
        self.mqtt_sender.subscribe("lamarrs/orchestrator", QoS::AtMostOnce).await.unwrap();
        loop {
            while let Ok(message) = self.mqtt_receiver.poll().await {
                match message{
                    Event::Incoming(Packet::Publish(packet)) => {
                        self.on_mqtt_published(packet).await;


                    }
                    _ => panic!("Unsupported message type received"),
                }
            }
        }
    }

    pub async fn on_mqtt_published(&mut self, packet: Publish) {
        match serde_json::from_slice(&packet.payload) {
            Ok::<OrchestratorMessage, serde_json::Error>(message) => {
                match message {
                    OrchestratorMessage::SendSubtitle(subtitles_with_target) => {
                        self.subtitles.send(SubtitleMessage::SendSubtitle(subtitles_with_target)).await;
                    },
                    OrchestratorMessage::SendColor(color_with_target) => {
                        self.color.send(ColorMessage::SendColor(color_with_target)).await;
                    },
                    OrchestratorMessage::Error(_) => todo!(),
                }
            },
            Err( .. ) => {}
        };

    }
}