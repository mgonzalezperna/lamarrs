pub mod sound_streamers;
pub mod text_streamers;

pub mod payload {
    use lamarrs_utils::enums::{GatewayMessage, RelativeLocation};
    use tokio::sync::mpsc::Sender;
    use uuid::Uuid;

    /// SubscriptionData must stay in this workspace because it depends on Tokio. Moving this to the lamarrs-utils doesn't make sense because
    /// a- It is not needed in any other workspace
    /// b- Would force lamarrs-utils to have Tokio as dependency, which would break web-ui as Tokio doesn't play nice with WASM.
    #[derive(Clone, Debug)]
    pub struct SusbcriptionData {
        pub sender_id: Uuid,
        pub sender: Sender<GatewayMessage>,
        pub location: RelativeLocation,
    }
}
