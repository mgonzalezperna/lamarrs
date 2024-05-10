use std::env;
mod server;

#[tokio::main]
async fn main() -> Result<(), server::ServerError> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    server::spawn(addr).await
}