use std::{
    str::FromStr,
    thread,
    time::{self, Duration},
};

use inquire::{CustomType, InquireError, Select};
use lamarrs_utils::{
    action_messages::{Action, Event},
    orchestration_messages::OrchestrationMessage,
    AudioFile, ColourRgb, RelativeLocation, Service, Subtitles,
};
use lipsum::lipsum_words_with_rng;
use rand::seq::{IndexedRandom, SliceRandom};
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
    let host = "192.168.178.70";
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
            let rnd_service = services.choose(&mut rand::rng()).unwrap();
            //let rnd_location = locations.choose(&mut rand::rng()).unwrap();
            let rnd_location = None;
            // I don't care if the subs are not random anymore. May fix it later, perhaps.
            let subtitles = heapless::String::try_from(lipsum::lipsum(5).as_str()).unwrap();
            let orchestrator_message: OrchestrationMessage = match rnd_service {
                Service::Subtitle => OrchestrationMessage::Request(
                    Event::PerformAction(Action::ShowNewSubtitles(Subtitles { subtitles })),
                    rnd_location.to_owned(),
                ),
                Service::Colour => OrchestrationMessage::Request(
                    Event::PerformAction(Action::ChangeColour(ColourRgb {
                        r: rand::random_range(0..=255),
                        g: rand::random_range(0..=255),
                        b: rand::random_range(0..=255),
                    })),
                    rnd_location.to_owned(),
                ),
                _ => break,
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
    // let target_location: Result<RelativeLocation, InquireError> =
    //     Select::new("What's the target location", locations).prompt();
    // if let Err(_) = target_location {
    //     panic!("There was an error processing the target location!");
    // }
    let target_location = None;

    let orchestrator_message: OrchestrationMessage = match selected_service {
        Ok(Service::Subtitle) => on_subtitle(target_location),
        Ok(Service::Colour) => on_color(target_location),
        Ok(Service::AudioPlayer) => on_play_audio(target_location),
        Err(_) => panic!("There was an error, please try again"),
    };
    send_to_mqtt(mqtt_sender, orchestrator_message);
    while let Some(Ok(notification)) = mqtt_receiver.iter().next() {
        info!("MQTT results= {:?}", notification);
    }
}

#[instrument(name = "Orchestrator::on_subtitle", level = "INFO", ret)]
fn on_subtitle(target_location: Option<RelativeLocation>) -> OrchestrationMessage {
    let requested_subtitles= CustomType::<String>::new("Subtitles to be sent:")
    .with_error_message("Subtitles with more than 50 chars can't be sent.")
    .with_help_message("A String of characters to be sent to the targetted devices. Must be shorter than 50 characters.")
    .prompt();
    let subtitles = heapless::String::try_from(requested_subtitles.unwrap().as_str()).unwrap();
    OrchestrationMessage::Request(
        Event::PerformAction(Action::ShowNewSubtitles(Subtitles { subtitles })),
        target_location,
    )
}

#[instrument(name = "Orchestrator::on_color", level = "INFO", ret)]
fn on_color(target_location: Option<RelativeLocation>) -> OrchestrationMessage {
    let red = CustomType::<u8>::new("Red:")
        .with_error_message("Red must have a value between 0 and 255.")
        .with_help_message("The value for Red in the RGB message to be sent.")
        .prompt();
    let green = CustomType::<u8>::new("Green:")
        .with_error_message("Green must have a value between 0 and 255.")
        .with_help_message("The value for Green in the RGB message to be sent.")
        .prompt();
    let blue = CustomType::<u8>::new("Blue:")
        .with_error_message("Blue must have a value between 0 and 255.")
        .with_help_message("The value for Blue in the RGB message to be sent.")
        .prompt();

    OrchestrationMessage::Request(
        Event::PerformAction(Action::ChangeColour(ColourRgb {
            r: red.unwrap(),
            g: green.unwrap(),
            b: blue.unwrap(),
        })),
        target_location.to_owned(),
    )
}

#[instrument(name = "Orchestrator::on_subtitle", level = "INFO", ret)]
fn on_play_audio(target_location: Option<RelativeLocation>) -> OrchestrationMessage {
    let requested_audio_file_with_extension= CustomType::<String>::new("Audio file to be played. INCLUDE EXTENSION:")
    .with_error_message("Audio file name with more than 55 chars can't be sent.")
    .with_help_message("A String of characters to be sent to the targetted devices. Must be shorter than 55 characters.")
    .prompt()
    .unwrap();
    let parsed_request: Vec<&str> = requested_audio_file_with_extension.split(".").collect();
    let file_name = heapless::String::try_from(parsed_request[0]).unwrap();
    let file_extension = heapless::String::try_from(parsed_request[1]).unwrap();

    OrchestrationMessage::Request(
        Event::PerformAction(Action::PlayAudio(AudioFile {
            file_name,
            file_extension,
        })),
        target_location,
    )
}

// #[instrument(name = "Orchestrator::on_color", level = "INFO", ret)]
// fn on_midi(target_location: RelativeLocation) -> OrchestratorMessage {
//     let midi_channel: Vec<u8> = (0..15).collect();
//     let midi_key: Vec<u8> = (0..127).collect();
//     let midi_vel: Vec<u8> = (0..100).collect();
//     let selected_channel: Result<u8, InquireError> =
//         Select::new("MIDI Channel targetted:", midi_channel).prompt();
//     let selected_key: Result<u8, InquireError> = Select::new("MIDI note:", midi_key).prompt();
//     let selected_vel: Result<u8, InquireError> = Select::new("MIDI velocity:", midi_vel).prompt();

//     match (selected_channel, selected_key, selected_vel) {
//         (Ok(channel), Ok(key), Ok(vel)) => OrchestratorMessage::SendMidi(SendMidiEvent {
//             event: MidiEvent::NoteOn { channel, key, vel },
//             target_location,
//         }),
//         _ => panic! {"There was an error building the MIDI message to be sent"},
//     }
// }

#[instrument(
    name = "Orchestrator::send_to_mqtt",
    skip(mqtt_sender),
    level = "INFO",
    ret
)]
fn send_to_mqtt(mqtt_sender: Client, orchestrator_message: OrchestrationMessage) {
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
