use defmt::{error, warn};
use embassy_net::{IpEndpoint, tcp::client};
use embassy_rp::clocks::RoscRng;
use embassy_time::Duration;
use heapless::{String, Vec};
use lamarrs_utils::{
    ClientIdAndLocation, Service, action_messages::Event, exchange_messages::ExchangeMessage
};
use postcard::to_vec;
use uuid::Builder;

use crate::{
    websocket_handler::{WebSocket, WsError},
    OledEvents, OLED_CHANNEL,
};

/// Lamarrs websocket handler.
/// It connects to the target server, upgrades the connection to a Websocket,
/// does the initial client base registration and subscribes to Color service.
/// WIP.
#[embassy_executor::task]
pub async fn server_handler(stack: embassy_net::Stack<'static>, target: IpEndpoint) {
    let mut ws_reading_buffer = [0u8; 256];
    let oled_sender = OLED_CHANNEL.sender();

    let mut rx_buffer: [u8; 4096] = [0u8; 4096];
    let mut tx_buffer: [u8; 4096] = [0u8; 4096];
    // Connects and upgrades to websocket.
    let mut websocket = loop {
        // Inner block: borrow buffers here only.
        // This avoid the problem of borrowing &mut rx_buffer and &mut tx_buffer into WebSocket::connect(...)
        // When `connect` is called, the async call holds those mutable references across an .await.
        // If the call fails and the loop runs again, the compiler thinks the previous borrow might still be active.
        // Thus, the compiler screams at you in order to prevent you from reusing the same buffers in the next loop iteration.
        // The block ensures the borrow lives only during the call. When the block ends, the borrow ends too.
        let websocket_handshake_result = {
           WebSocket::connect(stack, &mut rx_buffer, &mut tx_buffer, target).await  
        };
        match websocket_handshake_result {
            Ok(websocket) => break websocket,
            Err(_) => {
                warn!("WebSocket connect failed, retrying in 3s...");
                embassy_time::Timer::after(Duration::from_secs(3)).await; // Implement some sort of backoff mechanism.
            }
        }
    };

    // Notifies the screen that it is connected to lamarrs.
    oled_sender
        .send(OledEvents::ConnectedToOrchestrator(true))
        .await;

    // Creates the Client ID data for this session. This will be used to identify itself to the Server.
    let mut rng = RoscRng;
    let mut random_bytes = [0u8; 16];
    rng.fill_bytes(&mut random_bytes);
    let client_id = ClientIdAndLocation {
        id: Builder::from_random_bytes(random_bytes).into_uuid(),
        location: None,
    };
    // Sends initial basic registration message to lamarrs server.
    defmt::info!("Sending Register request to lamarrs server.");
    let lamarrs_message = ExchangeMessage::Request(Event::Register(client_id.clone()));
    send_message_to_lamarrs_server_and_process_response(&mut websocket, &lamarrs_message).await;

    // Send subscription to color service.
    defmt::info!("Subscribing to Colour service");
    let lamarrs_message = ExchangeMessage::Request(Event::SuscribeToService(Service::Colour, client_id));
    send_message_to_lamarrs_server_and_process_response(&mut websocket, &lamarrs_message).await;

    // Listener loop.
    loop {
        match websocket.recv_text(&mut ws_reading_buffer).await {
            Ok(payload) => {
                let message =
                    core::str::from_utf8(&ws_reading_buffer[..payload]).unwrap_or("<invalid>");
                oled_sender
                    .send(OledEvents::WsMessage(String::try_from(message).unwrap()))
                    .await;
                defmt::info!("Received: {}", message);
            }
            Err(e) => {
                defmt::warn!("Error receiving frame from lamarrs server: {:?}", e);
                panic!("Fatal Error");
            }
        }
    }
}

pub async fn send_message_to_lamarrs_server_and_process_response<'a> (
    websocket: &mut WebSocket<'a>,
    lamarrs_message: &ExchangeMessage
) {
    let mut ws_reading_buffer = [0u8; 256];

    // Preallocate an array to not send a payload too big.
    let mut buffer = [0u8; 125];

    // Serialize into array
    let payload_bytes = postcard::to_slice(&lamarrs_message, &mut buffer).unwrap();

    defmt::info!("Frame to be sent: {}", payload_bytes);

    match &websocket.send_bytes(payload_bytes).await {
        Ok(_) => {
            if let Ok(payload) = websocket.recv_text(&mut ws_reading_buffer).await {
                let msg =
                    core::str::from_utf8(&ws_reading_buffer[..payload]).unwrap_or("<invalid>");
                defmt::info!("Received: {}", msg);
            } else {
                error!("Lamarrs server rejected the message.");
            }
        }
        Err(e) => {
            defmt::warn!("Error while trying to register to lamarrs Server: {:?}", e);
        }
    }
}
