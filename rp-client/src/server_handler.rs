use core::str::FromStr;

use defmt::{error, info, warn};
use embassy_futures::select::select;
use embassy_net::IpEndpoint;
use embassy_rp::clocks::RoscRng;
use embassy_time::Duration;
use heapless::String;
use lamarrs_utils::{
    action_messages::{Action, Event},
    exchange_messages::ExchangeMessage,
    ClientIdAndLocation, Service,
};
use uuid::Builder;

use crate::{
    websocket_handler::{WebSocket, WsError},
    OledEvents, ASYNC_GPIO_INPUT_CHANNEL, OLED_CHANNEL,
};

/// Lamarrs websocket handler.
/// It connects to the target server, upgrades the connection to a Websocket,
/// does the initial client base registration and subscribes to Color service.
/// WIP.
#[embassy_executor::task]
pub async fn server_handler(stack: embassy_net::Stack<'static>, target: IpEndpoint) {
    let oled_sender = OLED_CHANNEL.sender();
    let async_gpio_receiver = ASYNC_GPIO_INPUT_CHANNEL.receiver();

    let mut rx_buffer: [u8; 4096] = [0u8; 4096];
    let mut tx_buffer: [u8; 4096] = [0u8; 4096];
    // Creates the Client ID data for this session. This will be used to identify itself to the Server.
    let mut rng = RoscRng;
    let mut random_bytes = [0u8; 16];
    rng.fill_bytes(&mut random_bytes);
    let client_id = ClientIdAndLocation {
        uuid: Builder::from_random_bytes(random_bytes).into_uuid(),
        location: None,
    };
    let mut uuid_buffer = [0u8; 36];
    let uuid_str =
        String::from_str(client_id.uuid.hyphenated().encode_lower(&mut uuid_buffer)).unwrap();
    // Connects and upgrades to websocket.
    loop {
        // Inner block: borrow buffers here only.
        // This avoid the problem of borrowing &mut rx_buffer and &mut tx_buffer into WebSocket::connect(...)
        // When `connect` is called, the async call holds those mutable references across an .await.
        // If the call fails and the loop runs again, the compiler thinks the previous borrow might still be active.
        // Thus, the compiler screams at you in order to prevent you from reusing the same buffers in the next loop iteration.
        // The block ensures the borrow lives only during the call. When the block ends, the borrow ends too.
        let websocket_handshake_result =
            { WebSocket::connect(stack, &mut rx_buffer, &mut tx_buffer, target).await };
        match websocket_handshake_result {
            Ok(mut websocket) => {
                // Notifies the screen that it is connected to lamarrs.
                oled_sender
                    .send(OledEvents::ConnectedToLamarrs(true, None))
                    .await;

                // Sends initial basic registration message to lamarrs server.
                defmt::info!("Sending Register request to lamarrs server.");
                let lamarrs_message = ExchangeMessage::Request(Event::Register(client_id.clone()));
                send_message_to_lamarrs_server_and_process_response(
                    &mut websocket,
                    &lamarrs_message,
                )
                .await;
                oled_sender
                    .send(OledEvents::RegiteredWithUuid(uuid_str.clone()))
                    .await;

                // Send subscription to color service.
                defmt::info!("Subscribing to Colour service");
                let lamarrs_message = ExchangeMessage::Request(Event::SuscribeToService(
                    Service::Colour,
                    client_id.clone(),
                ));
                send_message_to_lamarrs_server_and_process_response(
                    &mut websocket,
                    &lamarrs_message,
                )
                .await;

                loop {
                    let mut ws_reading_buffer = [0u8; 256];
                    let websocket_listener = websocket.recv_message(&mut ws_reading_buffer);
                    let gpio_input_listener = async_gpio_receiver.receive();
                    // Listener loop.
                    match select(websocket_listener, gpio_input_listener).await {
                        // Manage new message from the server.
                        embassy_futures::select::Either::First(ws_message) => {
                            match ws_message {
                                Ok(_) => {
                                    let message: ExchangeMessage =
                                        postcard::from_bytes(&ws_reading_buffer).unwrap();
                                    oled_sender
                                        .send(OledEvents::WsMessage(message.clone()))
                                        .await;
                                    
                                    // This buffer will be used by certain structs to show themselves as &str.
                                    // By now only Action implement the `as_str` function, but later we will
                                    // implement them for all as a Trait.
                                    let mut write_buffer = String::<128>::new();
                                    match message {
                                        ExchangeMessage::Ack(ack_result) => info!("Last request was successful!"),
                                        ExchangeMessage::Nack(nack_result) => warn!("Last request was not accepted by the server"),
                                        ExchangeMessage::Scene(event) => {
                                            match event {
                                                Event::PerformAction(action) => info!("New action requested: {:?}", action.as_str(&mut write_buffer)),
                                                _ => unreachable!("The Event requested is not compatible with Scene messages: {:?}", event)
                                            }
                                        }
                                        ExchangeMessage::Error(error_description) => error!("An error was reported by the server: {:?}", error_description.error_descr),
                                        ExchangeMessage::Heartbeat => {
                                            info!("Watchdog send a heartbeat request");
                                            let heartbeat_response = ExchangeMessage::HeartbeatAck;
                                            send_message_to_lamarrs_server_and_process_response(&mut websocket, &heartbeat_response).await;
                                        },
                                        _ => error!("Received an invalid Exchange Message.")
                                    }
                                    info!("Received message from Server");
                                }
                                Err(e) => {
                                    defmt::warn!(
                                        "Error receiving frame from lamarrs server: {:?}",
                                        e
                                    );
                                    // Fatal: break to reconnect or close clearly. This avoids getting trapped in an infinite logging loop.
                                    if let WsError::InvalidResponse = e {
                                        oled_sender.send(OledEvents::ConnectedToLamarrs(false, None)).await;
                                        break;
                                    }
                                }
                            }
                        }
                        // Manage internal messages from gpio input triggers.
                        embassy_futures::select::Either::Second(gpio_input_message) => {
                            defmt::info!("{}", gpio_input_message);
                            let lamarrs_message = match gpio_input_message {
                                crate::GpioInputEvents::NextTrigger => {
                                    defmt::info!(
                                        "Sending Request for NEXT scene to lamarrs server"
                                    );
                                    ExchangeMessage::NextScene
                                }
                                crate::GpioInputEvents::Retrigger => {
                                    defmt::info!(
                                        "Sending Request for RETRIGGER scene to lamarrs server"
                                    );
                                    ExchangeMessage::RetriggerScene
                                }
                            };
                            send_message_to_lamarrs_server_and_process_response(
                                &mut websocket,
                                &lamarrs_message,
                            )
                            .await;
                        }
                    }
                }
            }
            Err(_) => {
                warn!("WebSocket connection failed, retrying in 3s...");
                embassy_time::Timer::after(Duration::from_secs(3)).await; // Implement some sort of backoff mechanism.
            }
        };
    }
}

pub async fn send_message_to_lamarrs_server_and_process_response<'a>(
    websocket: &mut WebSocket<'a>,
    lamarrs_message: &ExchangeMessage,
) {
    let mut ws_reading_buffer = [0u8; 256];

    // Preallocate an array to not send a payload too big.
    let mut buffer = [0u8; 125];

    // Serialize into array
    let payload_bytes = postcard::to_slice(&lamarrs_message, &mut buffer).unwrap();

    defmt::info!("Frame to be sent: {}", payload_bytes);

    &websocket.send_bytes(payload_bytes).await;
}
