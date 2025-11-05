#![allow(non_snake_case)]

pub mod midi_processor;
pub mod websocket;

use dioxus::prelude::*;
use futures_util::StreamExt;
use lamarrs_utils::{
    enums::{ClientMessage, RelativeLocation, Service},
    midi_event::{self, MidiEvent},
};
use log::{debug, error, info};
use tracing::field::debug;
use uuid::Uuid;
const _TAILWIND_URL: &str = manganis::mg!(file("assets/tailwind.css"));

fn main() {
    // Init logger
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
    launch(App);
}

#[component]
fn App() -> Element {
    let uuid = Uuid::new_v4();
    let mut location = use_signal(|| RelativeLocation::Center);
    let background_color = use_signal(|| String::from("red"));
    let subtitle = use_signal(|| String::from(""));
    let sound_engine: Coroutine<MidiEvent> =
        use_coroutine(|mut rx: UnboundedReceiver<MidiEvent>| async move {
            let mut sound_handler: Option<midi_processor::Handle> = None;
            loop {
                while let Some(midi_event) = rx.next().await {
                    if let MidiEvent::SystemReset = midi_event {
                        sound_handler = Some(midi_processor::create_handler());
                    }
                    match &sound_handler {
                        Some(handler) => {
                            debug!("Playing note");
                            handler.send(midi_event);
                        }
                        None => {
                            error!("No sound context! MidiEvent can't be processed");
                            break;
                        }
                    }
                }
            }
        });
    let ws: Coroutine<ClientMessage> =
        use_coroutine(|mut rx: UnboundedReceiver<ClientMessage>| async move {
            let mut conn =
                websocket::WebsocketService::new(background_color, subtitle, sound_engine);
            let register = conn
                .sender
                .try_send(ClientMessage::Register((uuid, location.read().clone())));
            debug!("Register results {:?}", register);
            loop {
                while let Some(message) = rx.next().await {
                    debug!("Message received! {}", message);
                    debug!("Location is: {:?}", { location.read() });
                    conn.sender.try_send(message);
                }
            }
        });

    rsx! {
        div {
            style: "background-color: { background_color }",
            class: "w-full h-screen flex items-center justify-center",
            ul {
                class:"list-inside",
                // Subtitles button component, idk how to place it out of here because the ws used of the onchange event can't be passed as an argument, defeating the entire concept of this framework from hell.
                label {
                    class:"flex items-center relative w-max cursor-pointer select-none text-right",
                    span {
                        class:"text-lg font-bold mr-3 text-white forced-color-adjust-auto",
                        "Subscription  ",
                        span {
                            class: "before:block before:absolute before:-inset-1 before:-skew-y-3 before:bg-blue-500 relative inline-block",
                            span{
                                class: "relative text-white",
                                "Subtitles"
                            }
                        }
                        "  Service"
                    }
                    input {
                        class: "appearance-none transition-colors cursor-pointer w-14 h-7 rounded-full focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-black focus:ring-blue-500 bg-red-500 checked:bg-green-500 peer",
                        r#type: "checkbox",
                        onchange: move |_| ws.send(ClientMessage::Subscribe(Service::Subtitle)),
                    }
                    span {
                        class: "absolute font-medium text-xs uppercase right-1 text-white",
                        "OFF"
                    }
                    span {
                        class:"absolute font-medium text-xs uppercase right-8 text-white",
                        "ON"
                    }
                    span {
                        class:"w-7 h-7 right-7 absolute rounded-full transform transition-transform bg-gray-200 peer-checked:translate-x-7",
                    }
                }

                // Spacer
                div{
                    class: "h-5"
                }

                // Color button
                label {
                    class:"flex items-center relative w-max cursor-pointer select-none text-right",
                    span {
                        class:"text-lg font-bold mr-3 text-white forced-color-adjust-auto",
                        "Subscription  ",
                        span {
                            class: "before:block before:absolute before:-inset-1 before:-skew-y-3 before:bg-pink-500 relative inline-block",
                            span{
                                class: "relative text-white",
                                "Color"
                            }
                        }
                        "  Service"
                    }
                    input {
                        class: "appearance-none transition-colors cursor-pointer w-14 h-7 rounded-full focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-black focus:ring-blue-500 bg-red-500 checked:bg-green-500 peer",
                        r#type: "checkbox",
                        onchange: move |_| {
                            ws.send(ClientMessage::Subscribe(Service::Color));
                        }
                    }
                    span {
                        class: "absolute font-medium text-xs uppercase right-1 text-white",
                        "OFF"
                    }
                    span {
                        class:"absolute font-medium text-xs uppercase right-8 text-white",
                        "ON"
                    }
                    span {
                        class:"w-7 h-7 right-7 absolute rounded-full transform transition-transform bg-gray-200 peer-checked:translate-x-7",
                    }
                }

                // Spacer
                div{
                    class: "h-5"
                }

                // Sound button
                label {
                    class:"flex items-center relative w-max cursor-pointer select-none text-right",
                    span {
                        class:"text-lg font-bold mr-3 text-white forced-color-adjust-auto",
                        "Subscription  ",
                        span {
                            class: "before:block before:absolute before:-inset-1 before:-skew-y-3 before:bg-cyan-500 relative inline-block",
                            span{
                                class: "relative text-white",
                                "MIDI"
                            }
                        }
                        "  Service"
                    }
                    input {
                        class: "appearance-none transition-colors cursor-pointer w-14 h-7 rounded-full focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-black focus:ring-blue-500 bg-red-500 checked:bg-green-500 peer",
                        r#type: "checkbox",
                        onchange: move |_| {
                            ws.send(ClientMessage::Subscribe(Service::Midi));
                            sound_engine.send(MidiEvent::SystemReset)
                        }
                    }
                    span {
                        class: "absolute font-medium text-xs uppercase right-1 text-white",
                        "OFF"
                    }
                    span {
                        class:"absolute font-medium text-xs uppercase right-8 text-white",
                        "ON"
                    }
                    span {
                        class:"w-7 h-7 right-7 absolute rounded-full transform transition-transform bg-gray-200 peer-checked:translate-x-7",
                    }
                }

                // Spacer
                div{
                    class: "h-20"
                }

                fieldset {
                    class:"grid grid-cols-3 gap-2 rounded-xl bg-coral-200 text-lg font-bold mr-3 text-white forced-color-adjust-auto",
                    input {
                        id:"left",
                        class:"peer/left hidden",
                        r#type:"radio",
                        name:"status",
                        onclick: move |_| {
                            location.set(RelativeLocation::Left);
                            ws.send(ClientMessage::UpdateLocation(RelativeLocation::Left))
                        },
                    }
                    label {
                        r#for: "left",
                        class: "block cursor-pointer select-none rounded-xl p-2 text-center peer-checked/left:bg-blue-500 peer-checked/left:font-bold peer-checked/left:text-white",
                        "Left"
                    }
                    input {
                        id:"center",
                        class:"peer/center hidden",
                        r#type:"radio",
                        name:"status",
                        checked: true,
                        onclick: move |_| {
                            location.set(RelativeLocation::Center);
                            ws.send(ClientMessage::UpdateLocation(RelativeLocation::Center))
                        },
                    }
                    label {
                        r#for: "center",
                        class: "block cursor-pointer select-none rounded-xl p-2 text-center peer-checked/center:bg-blue-500 peer/center-checked:font-bold peer-checked/center:text-white",
                        "Center"
                    }
                    input {
                        id:"right",
                        class:"peer/right hidden",
                        r#type:"radio",
                        name:"status",
                        onclick: move |_| {
                            location.set(RelativeLocation::Right);
                            ws.send(ClientMessage::UpdateLocation(RelativeLocation::Right))
                        },
                    }
                    label {
                        r#for: "right",
                        class: "block cursor-pointer select-none rounded-xl p-2 text-center peer-checked/right:bg-blue-500 peer-checked/right:font-bold peer-checked/right:text-white",
                        "Right"
                    }
                }
                // Many spacers
                div {
                    class: "h-20"
                }
                div {
                    class: "h-20"
                }
                div {
                    class: "h-20"
                }
                // Subtitles
                div {
                    span {
                        class:"text-lg font-bold mr-3 text-white",
                        blockquote {
                            class: "absolute inset-x-0 bottom-0 before:block before:absolute before:-inset-1 before:bg-black relative inline-block text-2xl font-semibold italic text-center text-slate-900",
                            span{
                                class: "relative text-white",
                                "{subtitle}"
                            }
                        }
                    }
                }
            }
        }
    }
}
