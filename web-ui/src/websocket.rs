use dioxus::signals::{Signal, Writable};
use futures::{
    channel::mpsc::{Receiver, Sender},
    SinkExt, StreamExt,
};
use lamarrs_utils::enums::{GatewayError, GatewayMessage, SubscriberMessage};
use reqwasm::websocket::{futures::WebSocket, Message};

use wasm_bindgen_futures::spawn_local;

pub struct WebsocketService {
    pub sender: Sender<SubscriberMessage>,
}

impl WebsocketService {
    pub fn new(mut bg: Signal<String>, mut subs: Signal<String>) -> Self {
        let ws = WebSocket::open("ws://127.0.0.1:8080").unwrap();

        let (mut outgoing, mut incoming) = ws.split();

        let (sender, mut receiver) = futures::channel::mpsc::channel::<SubscriberMessage>(32);

        spawn_local(async move {
            while let Some(subscriber_message) = receiver.next().await {
                log::debug!("Sending message to Gateway {}", subscriber_message);
                outgoing
                    .send(Message::Text(
                        serde_json::to_string(&subscriber_message).unwrap(),
                    ))
                    .await
                    .unwrap();
            }
        });

        spawn_local(async move {
            log::info!("Waiting for Gateway messages...");
            loop {
                while let Some(msg) = incoming.next().await {
                    log::debug!("Processing new msg");
                    match msg {
                        Ok(Message::Text(payload)) => {
                            log::debug!("From Gateway: {}", payload);
                            match serde_json::from_str(&payload) {
                                Ok(GatewayMessage::Color(color)) => {
                                    log::info!("Request change of Color by Gateway: {}", color);
                                    bg.set(color.to_string());
                                }
                                Err(_) => todo!(),
                                _ => {
                                    log::info!("Notification Message Received: {}", payload);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("ERROR: {:?}", e)
                        }
                        _ => {
                            log::debug!("Weird message received from Gateway")
                        }
                    }
                }
                log::debug!("WebSocket Closed");
            }
        });

        Self { sender }
    }
}
