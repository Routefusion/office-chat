use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::crypto;
use crate::protocol::{Message, Packet};

pub const PORT: u16 = 7337;
pub const BROADCAST_ADDR: &str = "255.255.255.255";

/// Bind a UDP socket with SO_BROADCAST + SO_REUSEADDR.
pub async fn bind_socket() -> Arc<UdpSocket> {
    let addr: SocketAddr = format!("0.0.0.0:{PORT}").parse().unwrap();
    let std_sock = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .expect("failed to create socket");
    std_sock.set_reuse_address(true).expect("SO_REUSEADDR failed");
    #[cfg(target_os = "macos")]
    std_sock.set_reuse_port(true).expect("SO_REUSEPORT failed");
    std_sock.set_broadcast(true).expect("SO_BROADCAST failed");
    std_sock.set_nonblocking(true).expect("set_nonblocking failed");
    std_sock.bind(&addr.into()).expect("bind failed");

    Arc::new(UdpSocket::from_std(std_sock.into()).expect("tokio UdpSocket from_std failed"))
}

/// Send a message: serialize → encrypt → sign → broadcast.
pub async fn send_message(
    socket: &UdpSocket,
    key: &[u8; 32],
    signing_key: &SigningKey,
    msg: &Message,
) {
    let plaintext = bincode::serialize(msg).expect("message serialization failed");
    let (ciphertext, nonce) = crypto::encrypt(key, &plaintext);
    let signature = crypto::sign(signing_key, &ciphertext);
    let pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();

    let packet = Packet {
        ciphertext,
        nonce,
        signature,
        sender_pubkey: pubkey,
    };

    let data = packet.encode();
    let dest: SocketAddr = format!("{BROADCAST_ADDR}:{PORT}").parse().unwrap();
    if let Err(e) = socket.send_to(&data, dest).await {
        eprintln!("[net] send failed ({} bytes): {e}", data.len());
    }
}

/// Event from the network recv loop.
#[derive(Debug)]
pub struct IncomingMessage {
    pub message: Message,
    pub sender_pubkey: [u8; 32],
}

/// Receive loop: read UDP packets → decrypt → verify → forward to channel.
pub async fn recv_loop(
    socket: Arc<UdpSocket>,
    key: [u8; 32],
    own_pubkey: [u8; 32],
    tx: mpsc::Sender<IncomingMessage>,
) {
    let mut buf = [0u8; 2048];
    loop {
        let (len, _addr) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        let Some(packet) = Packet::decode(&buf[..len]) else {
            continue;
        };

        // Skip own messages
        if packet.sender_pubkey == own_pubkey {
            continue;
        }

        // Verify signature
        if !crypto::verify(&packet.sender_pubkey, &packet.ciphertext, &packet.signature) {
            continue;
        }

        // Decrypt
        let Some(plaintext) = crypto::decrypt(&key, &packet.ciphertext, &packet.nonce) else {
            continue; // wrong passphrase
        };

        let Ok(message) = bincode::deserialize::<Message>(&plaintext) else {
            continue;
        };

        let _ = tx
            .send(IncomingMessage {
                message,
                sender_pubkey: packet.sender_pubkey,
            })
            .await;
    }
}
