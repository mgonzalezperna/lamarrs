pub mod text_streamers;

pub mod payload {
    use lamarrs_utils::{
        enums::{Color, GatewayMessage, RelativeLocation},
        messages::Subtitle,
    };
    use tokio::sync::mpsc::Sender;
    use uuid::Uuid;

    #[derive(Clone, Debug)]
    pub struct SusbcriptionData {
        pub sender_id: Uuid,
        pub sender: Sender<GatewayMessage>,
        pub location: RelativeLocation,
    }

    #[derive(Clone, Debug)]
    pub struct SendSubtitle {
        pub subtitle: Subtitle,
        pub target_location: RelativeLocation,
    }

    #[derive(Clone, Debug)]
    pub struct SendColor {
        pub color: Color,
        pub target_location: RelativeLocation,
    }
}
