#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use core::fmt::Write;
use core::str::FromStr;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_net::{Config, IpAddress, IpEndpoint, Ipv4Address, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{
    self, Async, Config as Config_i2c, I2c, InterruptHandler as InterruptHandler_i2c,
};
use embassy_rp::peripherals::{DMA_CH0, I2C1, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel;

use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use heapless::String;
use ssd1306::mode::{BufferedGraphicsMode, DisplayConfig};
use ssd1306::prelude::{DisplayRotation, I2CInterface};
use ssd1306::size::DisplaySize128x64;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use static_cell::StaticCell;
use uuid::Builder;

mod websocket_client;
use websocket_client::WebSocket;

use {defmt_rtt as _, panic_probe as _, serde_json_core};

bind_interrupts!(struct Irqs {
    I2C1_IRQ => InterruptHandler_i2c<I2C1>;
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

const WIFI_NETWORK: &str = ""; // change to your network SSID
const WIFI_PASSWORD: &str = ""; // change to your network password

/// Events that worker tasks send to the OLED_CHANNEL.
enum OledEvents {
    ConnectedToWifi(bool),         // Connected stablished with router.
    ConnectedToOrchestrator(bool), // Connected to Lamarrs orchestrator.
    RegiteredWithUuid(String<20>), // Once registered, report the temporary lamarrs device UUID to the screen for easy identification.
    WsMessage(String<100>),        // New Message received from orchestrator.
}

/// Channel for oled/screen related messages
static OLED_CHANNEL: channel::Channel<CriticalSectionRawMutex, OledEvents, 10> =
    channel::Channel::new();

// Function that updates the SSD1306 OLED display.
fn update_line(
    display: &mut Ssd1306<
        I2CInterface<I2c<'_, I2C1, Async>>,
        DisplaySize128x64,
        BufferedGraphicsMode<DisplaySize128x64>,
    >,
    y: i32,
    text: &str,
    font_height: u32,
) {
    let rect = Rectangle::new(Point::new(0, y), Size::new(128, font_height));
    let clear = PrimitiveStyleBuilder::new()
        .fill_color(BinaryColor::Off)
        .build();
    rect.into_styled(clear).draw(display).unwrap();

    display.flush().unwrap();
    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();
    Text::with_baseline(text, Point::new(0, y), text_style, Baseline::Top)
        .draw(display)
        .unwrap();
    display.flush().unwrap();
}

// Worker that handles the SSD1306 OLED display.
#[embassy_executor::task]
async fn oled_ssd1306_task(i2c1: I2c<'static, I2C1, Async>) {
    defmt::info!("Oled screen initialising");
    let inbound = OLED_CHANNEL.receiver();

    let interface = I2CDisplayInterface::new(i2c1);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().unwrap();

    loop {
        // Do nothing until we receive any event
        let event = inbound.receive().await;
        match event {
            OledEvents::ConnectedToWifi(bool) => {
                let status = match bool {
                    true => "on",
                    false => "off",
                };
                let mut message: String<50> = String::new();
                write!(&mut message, "WiFi: {}", status).unwrap();
                update_line(&mut display, 0, message.as_str(), 10);
                defmt::debug!("Showing WiFi connection status in Oled");
            }
            OledEvents::ConnectedToOrchestrator(bool) => {
                let status = match bool {
                    true => "on",
                    false => "off",
                };
                let mut message: String<50> = String::new();
                write!(&mut message, "Lamarrs: {}", status).unwrap();
                update_line(&mut display, 16, message.as_str(), 10);
                defmt::debug!("Showing orchestrator connection status in Oled");
            }
            OledEvents::RegiteredWithUuid(uuid) => {
                let mut message: String<50> = String::new();
                write!(&mut message, "ID: {}", uuid).unwrap();
                update_line(&mut display, 32, message.as_str(), 10);
                defmt::debug!("Showing UUID in Oled");
            }
            OledEvents::WsMessage(payload) => {
                update_line(&mut display, 48, payload.as_str(), 10);
                defmt::debug!("Showing lamarrs payload in Oled");
            }
        }
    }
}

/// Create a new instance of the CYW43 driver.
#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

/// Start the Network stack.
#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

/// Lamarrs websocket handler.
/// It connects to the target server, upgrades the connection to a Websocket,
/// does the initial client base registration and subscribes to Color service.
/// WIP.
#[embassy_executor::task]
async fn ws_task(stack: embassy_net::Stack<'static>, target: IpEndpoint) {
    let mut rx_buffer: [u8; 4096] = [0u8; 4096];
    let mut tx_buffer: [u8; 4096] = [0u8; 4096];
    let mut uuid_buffer = [0u8; 64];
    let mut ws_reading_buffer = [0u8; 256];
    let oled_sender = OLED_CHANNEL.sender();

    // Connects and upgrades to websocket.
    let try_connect_websocket =
        WebSocket::connect(stack, &mut rx_buffer, &mut tx_buffer, target).await;
    let mut websocket =
        try_connect_websocket.expect("This should not fail, something went terribly wrong."); // Error handling is awful.
    oled_sender
        .send(OledEvents::ConnectedToOrchestrator(true))
        .await;

    // Sends initial basic registration message to lamarrs server.
    let mut message_to_orchestrator: String<512> = String::new();

    let mut rng = RoscRng;
    let mut random_bytes = [0u8; 16];
    rng.fill_bytes(&mut random_bytes);
    let client_id = Builder::from_random_bytes(random_bytes).into_uuid();

    write!(
        &mut message_to_orchestrator,
        "{{\"Register\":[\"{}\",\"Center\"]}}",
        client_id.as_hyphenated().encode_lower(&mut uuid_buffer)
    )
    .unwrap();

    match websocket.send_text(&message_to_orchestrator).await {
        Ok(_) => {
            defmt::info!("Successfully registered to lamarrs!");
            message_to_orchestrator = String::new();
            if let Ok(payload) = websocket.recv_text(&mut ws_reading_buffer).await {
                let msg =
                    core::str::from_utf8(&ws_reading_buffer[..payload]).unwrap_or("<invalid>");
                defmt::info!("Received: {}", msg);
            }
        }
        Err(e) => {
            defmt::warn!(
                "Error while trying to register to lamarrs orchestrator: {:?}",
                e
            );
        }
    }

    // Send subscription to color service.
    write!(&mut message_to_orchestrator, "{{\"Subscribe\":\"Color\"}}",).unwrap();
    match websocket.send_text(&message_to_orchestrator).await {
        Ok(_) => {
            defmt::info!("Successfully subscribed to color messages of lamarrs!");
            if let Ok(payload) = websocket.recv_text(&mut ws_reading_buffer).await {
                let msg =
                    core::str::from_utf8(&ws_reading_buffer[..payload]).unwrap_or("<invalid>");
                defmt::info!("Received: {}", msg);
            }
        }
        Err(e) => {
            defmt::warn!(
                "Error while trying to subscribe to color messages of lamarrs orchestrator: {:?}",
                e
            );
        }
    }

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
                defmt::warn!("Error receiving frame from lamarrs orchestrator: {:?}", e);
            }
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    defmt::info!("Booting up!");

    let peripherals = embassy_rp::init(Default::default());
    defmt::debug!("Peripherals initialised successfully.");

    // Set up I2C1 for the VL53L0X
    let i2c1 = i2c::I2c::new_async(
        peripherals.I2C1,
        peripherals.PIN_3,
        peripherals.PIN_2,
        Irqs,
        Config_i2c::default(),
    );
    spawner.spawn(oled_ssd1306_task(i2c1)).unwrap();

    // Initialise Oled.
    let oled_sender = OLED_CHANNEL.sender();
    oled_sender.send(OledEvents::ConnectedToWifi(false)).await;
    oled_sender
        .send(OledEvents::ConnectedToOrchestrator(false))
        .await;
    oled_sender.send(OledEvents::WsMessage(String::new())).await;

    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");
    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download 43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download 43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10140000
    //let fw = unsafe { core::slice::from_raw_parts(0x10100000 as *const u8, 230321) };
    //let clm = unsafe { core::slice::from_raw_parts(0x10140000 as *const u8, 4752) };

    let pwr = Output::new(peripherals.PIN_23, Level::Low);
    let cs = Output::new(peripherals.PIN_25, Level::High);
    let mut pio = Pio::new(peripherals.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        peripherals.PIN_24,
        peripherals.PIN_29,
        peripherals.DMA_CH0,
    );

    // Start the networking driver.
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    spawner.spawn(cyw43_task(runner)).unwrap();

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let config = Config::dhcpv4(Default::default());
    // Use static IP configuration instead of DHCP
    //let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
    //    address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24),
    //    dns_servers: Vec::new(),
    //    gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
    //});

    // Generate random seed
    let mut rng = RoscRng;
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(net_task(runner)).unwrap();

    while let Err(err) = control
        .join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
        .await
    {
        defmt::info!("Join failed with status={}", err.status);
    }

    defmt::info!("Waiting for link...");
    stack.wait_link_up().await;

    defmt::info!("Waiting for DHCP...");
    stack.wait_config_up().await;

    // And now we can use it!
    defmt::info!("Stack is up!");
    oled_sender.send(OledEvents::ConnectedToWifi(true)).await;

    let lamarrs_ip_address = Ipv4Address::from_str("").unwrap();
    let target = IpEndpoint::new(IpAddress::Ipv4(lamarrs_ip_address), 8080);
    spawner.spawn(ws_task(stack, target)).unwrap();
}
