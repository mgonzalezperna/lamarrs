use base64ct::Base64;
use base64ct::Encoding;
use core::fmt::Write;
use defmt::debug;
use defmt::info;
use embassy_net::tcp::ConnectError;
use embassy_net::tcp::Error as TcpError;
use embassy_net::tcp::TcpSocket;
use embassy_net::IpEndpoint;
use embassy_rp::clocks::RoscRng;
use heapless::{String, Vec};
use postcard::{from_bytes, to_vec};
use sha1::Digest;
use sha1::Sha1;
use {defmt_rtt as _, panic_probe as _};

#[derive(Debug, defmt::Format)]
pub enum WsError {
    Connect(ConnectError),
    Tcp(TcpError),
    HandshakeFailed,
    InvalidResponse,
    FrameTooLarge,
    Utf8,
}

impl From<TcpError> for WsError {
    fn from(e: TcpError) -> Self {
        WsError::Tcp(e)
    }
}

pub struct WebSocket<'a> {
    socket: TcpSocket<'a>,
}

impl<'a> WebSocket<'a> {
    /// Connects to a targetted Websocket server.
    pub async fn connect(
        stack: embassy_net::Stack<'static>,
        rx_buffer: &'a mut [u8; 4096],
        tx_buffer: &'a mut [u8; 4096],
        target: IpEndpoint,
    ) -> Result<Self, WsError> {
        let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
        socket.connect(target).await.map_err(WsError::Connect)?;

        // Generate Sec Websocket Key.
        // This is required by the WS spec: https://www.rfc-editor.org/rfc/rfc6455#page-24
        let mut key_bytes = [0u8; 16];
        let mut rng = RoscRng;
        rng.next_u64();
        rng.fill_bytes(&mut key_bytes);
        let mut enc_buf = [0u8; 128];
        let sec_ws_key = Base64::encode(&key_bytes, &mut enc_buf).unwrap();
        defmt::debug!("Generated Sec-WebSocket-Key: {:?}", sec_ws_key);

        // Build "Upgrade to Websocket" Request.
        let mut upgrade_to_websocket_request: String<512> = String::new();
        write!(
            &mut upgrade_to_websocket_request,
            "GET / HTTP/1.1\r\n\
             Host: localhost\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: {}\r\n\r\n",
            sec_ws_key
        )
        .unwrap();

        defmt::debug!(
            "Websocket Handshake request: {:?}",
            upgrade_to_websocket_request
        );
        socket
            .write(upgrade_to_websocket_request.as_bytes())
            .await
            .map_err(WsError::Tcp)?;
        socket.flush().await.map_err(WsError::Tcp)?;

        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).await.map_err(WsError::Tcp)?;
        let handshake_response =
            core::str::from_utf8(&buf[..n]).map_err(|_| WsError::InvalidResponse)?;
        defmt::debug!("Websocket Handshake response: {:?}", handshake_response);

        if !handshake_response.contains("101 Switching Protocols") {
            return Err(WsError::HandshakeFailed);
        }
        // According to the spec, the server will return to the client
        // a concatenation of the Sec-WebSocket-Key sent in the upgrade request
        // plus a GUID constant, both encoded in Base64.
        // Here we check the returned value matches the expected.

        // This is a constant given by the specification:
        // https://en.wikipedia.org/wiki/WebSocket#Opening_handshake
        const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let mut hasher = Sha1::new();
        hasher.update(sec_ws_key.as_bytes());
        hasher.update(GUID.as_bytes());
        let mut enc_buf = [0u8; 160];
        let accept = Base64::encode(&hasher.finalize(), &mut enc_buf).unwrap();

        if !handshake_response.contains(accept) {
            return Err(WsError::HandshakeFailed);
        }

        info!("Connection successfully established with lamarrs server!");
        Ok(WebSocket { socket })
    }

    /// Send to the server a Binary message.
    pub async fn send_bytes(&mut self, payload: &[u8]) -> Result<(), WsError> {
        debug!("Payload to be send is: {}, len: {}", payload, payload.len());
        // According to a faily small research I've made,
        // embedded systems or no_std implementations often handle frames in chunks (e.g.<= 4096 bytes)
        // to avoid large buffers.
        // The 125-byte limit only applies to the short (7-bit) payload length field.
        if payload.len() > 125 {
            return Err(WsError::FrameTooLarge);
        }

        // These are all based on https://en.wikipedia.org/wiki/WebSocket#Frame_structure

        // FIN + Opcode(text)
        // FIN = 1 → final frame (not fragmented)
        // RSV1–3 = 0
        // Opcode = 0x2 > Binary
        // The OR operator combines the bits.
        let b0 = 0x80u8 | 0x02u8;
        let mut header = [0u8; 6];
        header[0] = b0;
        // Similar to the previous entry, combines MASK + Payload length
        // Client > Server frames must always set MASK = 1
        header[1] = 0x80u8 | (payload.len() as u8);

        // Generates mask.
        // Random nonce. Present if the masked field is 1. The client generates a masking key for every masked frame.
        // https://en.wikipedia.org/wiki/WebSocket#Frame_structure
        let mut rng = RoscRng;
        let mut mask = [0u8; 4];
        rng.fill_bytes(&mut mask);
        defmt::debug!("The mask key is: {:?}", mask);
        header[2..6].copy_from_slice(&mask);

        // Apply mask
        // https://en.wikipedia.org/wiki/WebSocket#Client-to-server_masking
        let masked: Vec<u8, 512> = payload
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ mask[i % 4])
            .collect();

        self.socket.write(&header).await.map_err(WsError::Tcp)?;
        self.socket.write(&masked).await.map_err(WsError::Tcp)?;
        self.socket.flush().await.map_err(WsError::Tcp)?;
        Ok(())
    }

    /// Places the payload into the give reading_buffer, while it
    /// returns the usize for the slice of bytes to be read from the reading_buffer
    /// in order to get the text message received.
    pub async fn recv_message(&mut self, reading_buffer: &mut [u8]) -> Result<usize, WsError> {
        // As defined by the spec, a single frame with short 7-bit payload
        // must have a header with only 2 bytes.
        let mut header = [0u8; 2];
        self.socket
            .read(&mut header)
            .await
            .map_err(|_| WsError::InvalidResponse)?;

        // If FIN is not 0, it means this is a multiframe payload. We don´t support those. Err mng missing here though...
        let fin = header[0] & 0x80 != 0;
        let opcode = header[0] & 0x0F;

        // If opcode is not `2` (binary) or if the FIN is not 1,
        // it must fail, as the payload is not supported by this function.
        if !(opcode == 0x2 && fin) {
            return Err(WsError::InvalidResponse);
        }
        // Any frame coming from the server should NOT be masked, but since I can´t ensure that's the case with my server
        // implementation, I'll handle the mask and debug later if this is never used before removing it.
        let masked = header[1] & 0x80 != 0;
        let mut payload_len = (header[1] & 0x7F) as usize;

        if payload_len == 126 {
            let mut payload_data = [0u8; 2];
            self.socket
                .read(&mut payload_data)
                .await
                .map_err(|_| WsError::InvalidResponse)?;
            payload_len = u16::from_be_bytes(payload_data) as usize;
        } else if payload_len == 127 {
            // ignore big frames for simplicity
            return Err(WsError::FrameTooLarge);
        }

        // Very rough masking management.
        let mut mask = [0u8; 4];
        if masked {
            self.socket
                .read(&mut mask)
                .await
                .map_err(|_| WsError::InvalidResponse)?;
        }

        // This validation is here in case I have inconsistencies between
        // the buffer size given by the worker/task calling the function
        // and the payload received.
        if payload_len > reading_buffer.len() {
            return Err(WsError::FrameTooLarge);
        }

        // Read exactly `payload_len` bytes of payload data into the start of the buffer.
        let payload_bytes = self
            .socket
            .read(&mut reading_buffer[..payload_len])
            .await
            .map_err(|_| WsError::InvalidResponse)?;

        if masked {
            for i in 0..payload_bytes {
                reading_buffer[i] ^= mask[i % 4];
            }
        }

        Ok(payload_bytes)
    }
}
