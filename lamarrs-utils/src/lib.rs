pub mod error;
pub mod enums;
pub mod messages;

use tokio::io::AsyncReadExt;

// Our helper method which will read data from stdin and send it along the
// sender provided.
pub async fn read_stdin() -> Vec<u8> {
    let mut stdin = tokio::io::stdin();
    let mut buf = vec![0; 1024];
    let n = match stdin.read(&mut buf).await {
        Err(_) | Ok(0) => panic!(),
        Ok(n) => n,
    };
    buf.truncate(n);
    buf
}

