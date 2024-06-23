use std::time::Duration;

use inquire::{CustomType, InquireError, Select};
use lamarrs_utils::{
    enums::{Color, OrchestratorMessage, RelativeLocation, Service},
    error,
    messages::{SendColor, SendSubtitle, Subtitle},
};
use rumqttc::{mqttbytes::QoS, Client, MqttOptions};
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
    let (mut mqtt_sender, mut mqtt_receiver) = Client::new(mqttoptions, 10);
    mqtt_sender
        .subscribe("lamarrs/orchestrator", QoS::AtMostOnce)
        .unwrap();

    let service: Vec<Service> = Service::iter().collect::<Vec<_>>();
    let locations: Vec<RelativeLocation> = RelativeLocation::iter().collect::<Vec<_>>();
    let selected_service: Result<Service, InquireError> =
        Select::new("Select service to orchestrate", service).prompt();
    let target_location: Result<RelativeLocation, InquireError> =
        Select::new("What's the target location", locations).prompt();

    if let Err(_) = target_location {
        panic!("There was an error processing the target location!");
    }

    let orchestrator_message: OrchestratorMessage = match selected_service {
        Ok(Service::Subtitle) => on_subtitle(target_location.unwrap()),
        Ok(Service::Color) => on_color(target_location.unwrap()),
        Err(_) => panic!("There was an error, please try again"),
    };

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
        Select::new("What's the target location", colors).prompt();

    match selected_color {
        Ok(color) => OrchestratorMessage::SendColor(SendColor {
            color,
            target_location,
        }),
        Err(_) => panic! {"There was an error building the color message to be sent"},
    }
}
