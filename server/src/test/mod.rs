pub mod fake_subscriber;
use std::{thread, time::Duration};

use crate::{
    services::{payload, text_streamers::{ColorMessage, ColorStreamer, SubtitlesStreamer}},
    ws_factory::SubscriberBuilder,
    ServerError,
};
use fake_subscriber::FakeSubscriber;
use futures_util::future::join_all;
use lamarrs_utils::enums::{
    Color, GatewayError, GatewayMessage, RegisterResult, RelativeLocation, Service, SubscribeResult, SubscriberMessage
};
use tokio::{net::TcpListener, sync::mpsc::error::TryRecvError};
use tracing::{debug, error, info, instrument, trace, warn};
use url::Url;

#[cfg(test)]
fn create_services() -> (SubtitlesStreamer, ColorStreamer) {
    let subtitle_service = SubtitlesStreamer::new();
    let color_service = ColorStreamer::new();
    (subtitle_service, color_service)
}

async fn start_tcp_stream() -> (TcpListener, Url) {
    // Start a listener in port 0, so the OS give us a random port
    let addr = format!("localhost:0");

    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    let port = listener.local_addr().unwrap().port();
    let url = Url::parse(&format!("ws://localhost:{port}")).unwrap();

    (listener, url)
}

async fn start_app(
    mut subtitle_service: SubtitlesStreamer,
    mut color_service: ColorStreamer,
    listener: TcpListener,
) -> Result<(), ServerError> {
    let ws_factory = SubscriberBuilder::new(
        subtitle_service.sender.clone(),
        color_service.sender.clone(),
    );
    tokio::select! {
        _ = subtitle_service.run() => {
            Err(ServerError::TextServiceError)
        }
        _ = color_service.run() => {
            Err(ServerError::TextServiceError)
        }
        _ = ws_factory.run(listener) => {
            Err(ServerError::WebSocketFactoryError)
        }
    }
}

#[test_log::test(tokio::test)]
async fn test_create_ws_for_new_client() {
    let (listener, url) = start_tcp_stream().await;
    let mut fake_client: FakeSubscriber = FakeSubscriber::new(url, RelativeLocation::Center).await;
    let (subtitle_service, color_service) = create_services();
    tokio::spawn(start_app(subtitle_service, color_service, listener));
    fake_client.start().await;
    fake_client.register().await;
    let result = fake_client.recv().await.unwrap();
    assert_eq!(
        GatewayMessage::RegisterResult(RegisterResult::Success),
        result
    );
}

#[test_log::test(tokio::test)]
async fn test_new_client_tries_to_subscribe_without_registering() {
    let (listener, url) = start_tcp_stream().await;
    let mut fake_client: FakeSubscriber = FakeSubscriber::new(url, RelativeLocation::Center).await;
    let (subtitle_service, color_service) = create_services();
    tokio::spawn(start_app(subtitle_service, color_service, listener));
    fake_client.start().await;
    fake_client
        .send(SubscriberMessage::Subscribe(Service::Subtitle))
        .await;
    let result = fake_client.recv().await;
    assert_eq!(
        GatewayMessage::Error(GatewayError::UnregisteredSubscriber),
        result.unwrap()
    );
}

#[test_log::test(tokio::test)]
async fn test_new_client_subscribes_to_subtitles() {
    let (listener, url) = start_tcp_stream().await;
    let mut fake_client: FakeSubscriber = FakeSubscriber::new(url, RelativeLocation::Center).await;
    let (subtitle_service, color_service) = create_services();
    tokio::spawn(start_app(subtitle_service, color_service, listener));
    fake_client.start().await;
    fake_client.register().await;
    let result = fake_client.recv().await.unwrap();
    assert_eq!(
        GatewayMessage::RegisterResult(RegisterResult::Success),
        result
    );
    fake_client
        .send(SubscriberMessage::Subscribe(Service::Subtitle))
        .await;
    let result = fake_client.recv().await.unwrap();
    assert_eq!(
        GatewayMessage::SubscribeResult(SubscribeResult::Success),
        result
    );
}

#[test_log::test(tokio::test)]
async fn test_new_client_subscribes_to_color() {
    let (listener, url) = start_tcp_stream().await;
    let mut fake_client: FakeSubscriber = FakeSubscriber::new(url, RelativeLocation::Center).await;
    let (subtitle_service, color_service) = create_services();
    tokio::spawn(start_app(subtitle_service, color_service, listener));
    fake_client.start().await;
    fake_client.register().await;
    let result = fake_client.recv().await.unwrap();
    assert_eq!(
        GatewayMessage::RegisterResult(RegisterResult::Success),
        result
    );
    fake_client
        .send(SubscriberMessage::Subscribe(Service::Color))
        .await;
    let result = fake_client.recv().await.unwrap();
    assert_eq!(
        GatewayMessage::SubscribeResult(SubscribeResult::Success),
        result
    );
}

#[test_log::test(tokio::test)]
async fn test_several_clients_connect_and_register() {
    let (listener, url) = start_tcp_stream().await;
    let mut fake_client_1_left: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Left).await;
    let mut fake_client_2_left: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Left).await;
    let mut fake_client_3_center: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Center).await;
    let mut fake_client_4_right: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Right).await;
    let mut fake_client_5_center: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Center).await;
    let list_fake_clients = vec![
        fake_client_1_left,
        fake_client_2_left,
        fake_client_3_center,
        fake_client_4_right,
        fake_client_5_center,
    ];
    let (subtitle_service, color_service) = create_services();

    tokio::spawn(start_app(subtitle_service, color_service, listener));
    let list_fake_clients = join_all(list_fake_clients.into_iter().map(|mut client| async move {
        client.start().await;
        client.register().await;
        client
    })).await;

    join_all(
        list_fake_clients.into_iter().map(|mut client| async move{
                let register_result = client.recv().await;
                assert_eq!(
                    GatewayMessage::RegisterResult(RegisterResult::Success),
                    register_result.unwrap()
                );
    })).await;
}


#[test_log::test(tokio::test)]
async fn test_several_clients_subscribe_to_color_different_locations_gets_different_messages() {
    let (listener, url) = start_tcp_stream().await;
    let mut fake_client_1_left: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Left).await;
    let mut fake_client_2_left: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Left).await;
    let mut fake_client_3_center: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Center).await;
    let mut fake_client_4_right: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Right).await;
    let mut fake_client_5_center: FakeSubscriber =
        FakeSubscriber::new(url.clone(), RelativeLocation::Center).await;
    let list_fake_clients = vec![
        fake_client_1_left,
        fake_client_2_left,
        fake_client_3_center,
        fake_client_4_right,
        fake_client_5_center,
    ];
    let (subtitle_service, color_service) = create_services();
    let color_service_sender = color_service.sender.clone();
    tokio::spawn(start_app(subtitle_service, color_service, listener));
    let list_fake_clients = join_all(list_fake_clients.into_iter().map(|mut client| async move {
        client.start().await;
        client.register().await;
        client.recv().await;
        client.send(SubscriberMessage::Subscribe(Service::Color)).await.expect("error subscribing");
        client.recv().await;
        client
    })).await;

    color_service_sender.send(ColorMessage::SendColor(payload::SendColor{color: Color::Red, target_location: RelativeLocation::Center})).await.expect("Error sending Color message");

    let list_fake_clients = join_all(list_fake_clients.into_iter().map(|mut client| async move{
        match client.location {
            RelativeLocation::Center => {
                let new_color = client.recv().await;
                assert_eq!(
                    GatewayMessage::Color(Color::Red),
                    new_color.unwrap()
                );
            },
            _ => {
                let empty = client.try_recv().await;
                assert_eq!(Err(TryRecvError::Empty), empty);    
            }
        }
        client
    })).await;

    color_service_sender.send(ColorMessage::SendColor(payload::SendColor{color: Color::Blue, target_location: RelativeLocation::Right})).await.expect("Error sending Color message");

    let list_fake_clients = join_all(list_fake_clients.into_iter().map(|mut client| async move{
        match client.location {
            RelativeLocation::Right=> {
                let new_color = client.recv().await;
                assert_eq!(
                    GatewayMessage::Color(Color::Blue),
                    new_color.unwrap()
                );
            },
            _ => {
                let empty = client.try_recv().await;
                assert_eq!(Err(TryRecvError::Empty), empty);    
            }
        }
        client
    })).await;

    color_service_sender.send(ColorMessage::SendColor(payload::SendColor{color: Color::White, target_location: RelativeLocation::Left})).await.expect("Error sending Color message");

    join_all(list_fake_clients.into_iter().map(|mut client| async move{
        match client.location {
            RelativeLocation::Left=> {
                let new_color = client.recv().await;
                assert_eq!(
                    GatewayMessage::Color(Color::White),
                    new_color.unwrap()
                );
            },
            _ => {
                let empty = client.try_recv().await;
                assert_eq!(Err(TryRecvError::Empty), empty);    
            }
        }
    })).await;

}
