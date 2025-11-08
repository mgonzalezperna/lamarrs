#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

mod oled_handler;
mod server_handler;
mod websocket_handler;

use core::str::FromStr;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_net::{Config, IpAddress, IpEndpoint, Ipv4Address, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::i2c::{self, Config as Config_i2c, InterruptHandler as InterruptHandler_i2c};
use embassy_rp::peripherals::{DMA_CH0, I2C1, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel;

use crate::oled_handler::{oled_ssd1306_task, OledEvents};
use heapless::String;
use lamarrs_utils::action_messages::Action;
use lamarrs_utils::exchange_messages::ExchangeMessage;
use lamarrs_utils::ClientIdAndLocation;
use serde::Serialize;
use ssd1306::mode::{BufferedGraphicsMode, DisplayConfig};
use ssd1306::prelude::{DisplayRotation, I2CInterface};
use ssd1306::size::DisplaySize128x64;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use static_cell::StaticCell;
use uuid::Builder;

mod server_handler;
mod websocket_handler;
use {defmt_rtt as _, panic_probe as _};

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
/// Channel for oled/screen related messages.
static OLED_CHANNEL: channel::Channel<CriticalSectionRawMutex, OledEvents, 10> =
    channel::Channel::new();
/// Channel for Button triggered messages.
static ASYNC_GPIO_INPUT_CHANNEL: channel::Channel<CriticalSectionRawMutex, GpioInputEvents, 10> =
    channel::Channel::new();

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
    spawner
        .spawn(server_handler::server_handler(stack, target))
        .unwrap();
}
