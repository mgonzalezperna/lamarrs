#![allow(non_snake_case)]

use std::{fmt::format, future::IntoFuture};
pub mod websocket;

use dioxus::prelude::*;
use futures::pin_mut;
use futures_util::{future, SinkExt, StreamExt, TryStreamExt};
use lamarrs_utils::enums::{RelativeLocation, Service, SubscriberMessage};
use log::{debug, error, info};
use uuid::{serde, Uuid};

fn main() {
    // Init logger
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
    launch(App);
}

#[component]
fn App() -> Element {
    let uuid = Uuid::new_v4();
    let location = RelativeLocation::Center;
    let background_color = use_signal(|| String::from("red"));
    let subtitle = use_signal(|| String::from(""));
    let ws: Coroutine<SubscriberMessage> =
        use_coroutine(|mut rx: UnboundedReceiver<SubscriberMessage>| async move {
            let mut conn = websocket::WebsocketService::new(background_color, subtitle);
            let register = conn
                .sender
                .try_send(SubscriberMessage::Register((uuid, location.clone())));
            debug!("Register results {:?}", register);
            loop {
                while let Some(message) = rx.next().await {
                    conn.sender.try_send(message);
                }
            }
        });

    rsx! {
       div {
            style: "left: 0; top: 0; position: fixed; width: 100%; height: 100%; background-color: { background_color }",
            id: "background",
            button { onclick: move |_| ws.send(SubscriberMessage::Subscribe(Service::Color)), "Subscribe to Color" }
       }
    }
}
