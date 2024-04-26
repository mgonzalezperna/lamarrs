use std::{
    collections::HashMap,
    env,
    fmt::Display,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use inquire::{InquireError, Select};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn handle_connection(peer_map: PeerMap, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    peer_map.lock().unwrap().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let messages_incoming = incoming.try_for_each(|msg| {
        println!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );
        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(messages_incoming, receive_from_others);
    future::select(messages_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.clone().lock().unwrap().remove(&addr);
}

async fn write_to_peers(peer_map: PeerMap) {
    loop {
        let stdin = read_stdin().await;
        let message = Message::Text(String::from_utf8(stdin.clone()).unwrap());
        let peers = peer_map.lock().unwrap();
        let broadcast_recipients = peers
            .iter()
            .map(|(_, ws_sink)| ws_sink);
        for recp in broadcast_recipients {
            recp.unbounded_send(message.clone()).unwrap();
        };
    };
}

async fn spawn_sever(addr: String) {
    let state=PeerMap::new(Mutex::new(HashMap::new()));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
        tokio::spawn(write_to_peers(state.clone()));
    }
}

async fn spawn_client(connect_addr: String) {
    let url = url::Url::parse(&connect_addr).expect("Problem parsing the URL.");

    let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
    tokio::spawn(async move{
        loop {
            let stdin = read_stdin().await;
            stdin_tx.unbounded_send(Message::binary(stdin)).unwrap();
        };
    });
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");

    let (write, read) = ws_stream.split();

    let stdin_to_ws = stdin_rx.map(Ok).forward(write);
    let ws_to_stdout = {
        read.for_each(|message| async {
            let data = message.unwrap().into_data();
            tokio::io::stdout().write_all(&data).await.unwrap();
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
async fn read_stdin() -> Vec<u8> {
    let mut stdin = tokio::io::stdin();
    let mut buf = vec![0; 1024];
    let n = match stdin.read(&mut buf).await {
        Err(_) | Ok(0) => panic!(),
        Ok(n) => n,
    };
    buf.truncate(n);
    buf
}

#[derive(Debug, EnumIter)]
enum Option {
    Server,
    Client,
}

impl Display for Option {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self {
            Option::Server => "Server",
            Option::Client => "Client",
        };
        write!(f, "{}", state)
    }
}

#[tokio::main]
async fn main() {
    let options: Vec<_> = Option::iter().collect();
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "localhost:8080".to_string());

    let ans: Result<Option, InquireError> =
        Select::new("What's your favorite fruit?", options).prompt();

    match ans {
        Ok(Option::Server) => spawn_sever(addr).await,
        Ok(Option::Client) => spawn_client("ws://".to_string() + &addr).await,
        Err(_) => panic!("Error has ocurred."),
    }
}
