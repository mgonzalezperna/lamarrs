use core::fmt::Write;
use core::panic;

use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C1;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use heapless::String;
use lamarrs_utils::action_messages::{Action, Event as ActionEvent};
use lamarrs_utils::exchange_messages::{AckResult, ExchangeMessage, NackResult};
use ssd1306::mode::{BufferedGraphicsMode, DisplayConfig};
use ssd1306::prelude::{DisplayRotation, I2CInterface};
use ssd1306::size::DisplaySize128x64;
use ssd1306::{I2CDisplayInterface, Ssd1306};

use crate::OLED_CHANNEL;

/// Events that worker tasks send to the OLED_CHANNEL.
pub enum OledEvents {
    ConnectedToWifi(bool),         // Connected stablished with router.
    ConnectedToOrchestrator(bool), // Connected to Lamarrs orchestrator.
    RegiteredWithUuid(String<36>), // Once registered, report the temporary lamarrs device UUID to the screen for easy identification.
    WsMessage(ExchangeMessage),    // New Message received from orchestrator.
}

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
pub async fn oled_ssd1306_task(i2c1: I2c<'static, I2C1, Async>) {
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
                write!(&mut message, "{}", uuid).unwrap();
                update_line(&mut display, 32, message.as_str(), 10);
                defmt::debug!("Showing UUID in Oled");
            }
            OledEvents::WsMessage(exchange_message) => {
                let message_to_show = match exchange_message {
                    ExchangeMessage::Ack(ack_result) => match ack_result {
                        AckResult::Success => "Success!",
                        AckResult::UpdatedSubscription => "Updated subscription",
                        AckResult::UpdatedLocation => "Updated location",
                    },
                    ExchangeMessage::Nack(nack_result) => match nack_result {
                        NackResult::AlreadySubscribed => "Rejected: Already subscribed",
                        NackResult::NotSubscribed => "Rejected: Not subscribed",
                        NackResult::Failed => "Failed",
                    },
                    ExchangeMessage::Scene(event) => {
                        if let ActionEvent::PerformAction(action) = event {
                            match action {
                                Action::ShowNewSubtitles(_) => "New subtitles",
                                Action::ChangeColour(_) => "New Colour",
                                Action::PlayAudio(_) => "Play audio file",
                            }
                        } else {
                            panic!("Invalid Scene event reported to the Oled screen.")
                        }
                    }
                    ExchangeMessage::Error(error_description) => {
                        &error_description.error_descr.clone()
                    }
                    ExchangeMessage::Heartbeat => "Heartbeat received",
                    _ => unreachable!("Oled received an unsuported message to show."),
                };
                update_line(&mut display, 48, message_to_show, 10);
                defmt::debug!("Showing lamarrs payload in Oled");
            }
        }
    }
}
