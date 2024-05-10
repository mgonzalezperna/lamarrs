use std::env;
mod client;

#[tokio::main]
async fn main() -> Result<(), client::ClientError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "localhost:8080".to_string());

    client::spawn("ws://".to_string() + &addr).await
}
