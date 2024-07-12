use std::{
    str::FromStr,
    thread,
    time::{self, Duration},
};

use inquire::{CustomType, InquireError, Select};
use lamarrs_utils::{
    enums::{Color, OrchestratorMessage, RelativeLocation, Service},
    error,
    messages::{SendColor, SendMidiEvent, SendSubtitle, Subtitle}, midi_event::MidiEvent,
};
use lipsum::lipsum_words_with_rng;
use rand::seq::SliceRandom;
use rumqttc::{mqttbytes::QoS, Client, Connection, EventLoop, MqttOptions};
use strum::{EnumIter, IntoEnumIterator};
use tracing::{info, instrument};
use tracing_subscriber::filter::{EnvFilter, ParseError};

fn configure_logging(level: &str) -> Result<(), ParseError> {
    let env_filter = EnvFilter::try_new(level)?.add_directive(
        "rumqttc=info"
            .parse()
            .expect("Failed to set log level of rumqttc to info"),
    );

    let format = tracing_subscriber::fmt::format()
        .with_source_location(true)
        .with_target(false);

    tracing_subscriber::fmt()
        .event_format(format)
        .with_env_filter(env_filter)
        .init();
    Ok(())
}
#[instrument(name = "Orchestrator::main", level = "INFO")]
fn main() {
    configure_logging("debug");
    let host = "localhost";
    let port: u16 = 1883;
    let mut mqttoptions = MqttOptions::new("lamarrs-orchestrator", host, port);
    mqttoptions.set_keep_alive(Duration::from_secs(5));
    let (mut mqtt_sender, mut mqtt_receiver) = Client::new(mqttoptions.clone(), 10);
    mqtt_sender
        .subscribe("lamarrs/orchestrator", QoS::AtMostOnce)
        .unwrap();

    let mode: Vec<&str> = vec!["Loop", "Single Message"];
    let services: Vec<Service> = Service::iter().collect::<Vec<_>>();
    let locations: Vec<RelativeLocation> = RelativeLocation::iter().collect::<Vec<_>>();
    let midi_channel: Vec<u8> = (0..15).collect();
    let midi_key: Vec<u8> = (30..90).collect();
    let midi_vel: Vec<u8> = (80..100).collect();

    if Select::new("Select mode for the orchestrator", mode)
        .prompt()
        .unwrap()
        == "Loop"
    {
        info!("Entering Loop mode, to exit please press Ctrl+C");
        loop {
            let rnd_service = services.choose(&mut rand::thread_rng()).unwrap();
            let rnd_location = locations.choose(&mut rand::thread_rng()).unwrap();
            let orchestrator_message: OrchestratorMessage = match rnd_service {
                Service::Subtitle => OrchestratorMessage::SendSubtitle(SendSubtitle {
                    subtitle: Subtitle::from_str(
                        lipsum_words_with_rng(&mut rand::thread_rng(), 3).as_str(),
                    )
                    .unwrap(),
                    target_location: rnd_location.to_owned(),
                }),
                Service::Color => OrchestratorMessage::SendColor(SendColor {
                    color: Color::iter()
                        .collect::<Vec<_>>()
                        .choose(&mut rand::thread_rng())
                        .unwrap()
                        .to_owned(),
                    target_location: rnd_location.to_owned(),
                }),
                Service::Midi => OrchestratorMessage::SendMidi(SendMidiEvent{
                    event: MidiEvent::NoteOn {
                        channel: 0, //*midi_channel.choose(&mut rand::thread_rng()).unwrap(), 
                        key: *midi_key.choose(&mut rand::thread_rng()).unwrap(),
                        vel: *midi_vel.choose(&mut rand::thread_rng()).unwrap(),
                    },
                    target_location: rnd_location.to_owned(),
                })
            };
            send_to_mqtt(mqtt_sender.clone(), orchestrator_message);
            while let Some(Ok(notification)) = mqtt_receiver.iter().next() {
                info!("MQTT results= {:?}", notification);
            }
            let ten_millis = time::Duration::from_millis(200);
            thread::sleep(ten_millis);
        }
    }

    let selected_service: Result<Service, InquireError> =
        Select::new("Select service to orchestrate", services).prompt();
    let target_location: Result<RelativeLocation, InquireError> =
        Select::new("What's the target location", locations).prompt();

    if let Err(_) = target_location {
        panic!("There was an error processing the target location!");
    }

    let orchestrator_message: OrchestratorMessage = match selected_service {
        Ok(Service::Subtitle) => on_subtitle(target_location.unwrap()),
        Ok(Service::Color) => on_color(target_location.unwrap()),
        Ok(Service::Midi) => on_midi(target_location.unwrap()),
        Err(_) => panic!("There was an error, please try again"),
    };
    send_to_mqtt(mqtt_sender, orchestrator_message);
    while let Some(Ok(notification)) = mqtt_receiver.iter().next() {
        info!("MQTT results= {:?}", notification);
    }
}

#[instrument(name = "Orchestrator::on_subtitle", level = "INFO", ret)]
fn on_subtitle(target_location: RelativeLocation) -> OrchestratorMessage {
    let subtitles= CustomType::<Subtitle>::new("Subtitles to be sent:")
    .with_error_message("Subtitles with more than 35 chars can't be sent.")
    .with_help_message("A String of characters to be sent to the targetted devices. Must be shorter than 35 characters.")
    .prompt();

    match subtitles {
        Ok(subtitle) => OrchestratorMessage::SendSubtitle(SendSubtitle {
            subtitle,
            target_location,
        }),
        Err(_) => panic! {"There was an error building the subtitles message to be sent"},
    }
}

#[instrument(name = "Orchestrator::on_color", level = "INFO", ret)]
fn on_color(target_location: RelativeLocation) -> OrchestratorMessage {
    let colors: Vec<Color> = Color::iter().collect::<Vec<_>>();
    let selected_color: Result<Color, InquireError> =
        Select::new("Color to be sent:", colors).prompt();

    match selected_color {
        Ok(color) => OrchestratorMessage::SendColor(SendColor {
            color,
            target_location,
        }),
        Err(_) => panic! {"There was an error building the color message to be sent"},
    }
}

#[instrument(name = "Orchestrator::on_color", level = "INFO", ret)]
fn on_midi(target_location: RelativeLocation) -> OrchestratorMessage {
    let midi_channel: Vec<u8> = (0..15).collect();
    let midi_key: Vec<u8> = (0..127).collect();
    let midi_vel: Vec<u8> = (0..100).collect();
    let selected_channel: Result<u8, InquireError> =
        Select::new("MIDI Channel targetted:", midi_channel).prompt();
    let selected_key: Result<u8, InquireError> =
        Select::new("MIDI note:", midi_key).prompt();
    let selected_vel: Result<u8, InquireError> =
        Select::new("MIDI velocity:", midi_vel).prompt();


    match (selected_channel, selected_key, selected_vel) {
        (Ok(channel), Ok(key), Ok(vel)) => OrchestratorMessage::SendMidi(SendMidiEvent {
            event: MidiEvent::NoteOn { channel, key, vel },
            target_location,
        }),
        _ => panic! {"There was an error building the MIDI message to be sent"},
    }
}

#[instrument(
    name = "Orchestrator::send_to_mqtt",
    skip(mqtt_sender),
    level = "INFO",
    ret
)]
fn send_to_mqtt(mqtt_sender: Client, orchestrator_message: OrchestratorMessage) {
    info!(
        "Message to be sent via MQTT {}",
        serde_json::to_string(&orchestrator_message)
            .unwrap()
            .to_string()
    );
    let sending_results = mqtt_sender.publish(
        "lamarrs/orchestrator",
        QoS::AtLeastOnce,
        false,
        serde_json::to_string(&orchestrator_message).unwrap(),
    );
    info!(?sending_results, "Sending results");
    mqtt_sender.disconnect();
}
