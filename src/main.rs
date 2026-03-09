mod crypto;
mod history;
mod net;
mod protocol;
mod state;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use ed25519_dalek::SigningKey;
use tokio::sync::mpsc;

use crate::history::HistoryEntry;
use crate::net::IncomingMessage;
use crate::protocol::{Message, MAX_TEXT_LEN};
use crate::state::PeerState;
use crate::ui::Ui;

#[derive(Parser)]
#[command(name = "office-chat", about = "Encrypted LAN chat over UDP broadcast")]
struct Args {
    /// Your display nickname
    #[arg(short, long)]
    nick: String,

    /// Shared passphrase for encryption
    #[arg(short, long)]
    passphrase: String,

    /// Number of history messages to load on startup
    #[arg(long, default_value = "50")]
    history: usize,
}

fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home directory")
        .join(".office-chat")
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let data = data_dir();

    // Load or generate signing keypair
    let signing_key: SigningKey = crypto::load_or_generate_keypair(&data.join("keypair.bin"));
    let own_pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();

    // Derive symmetric key from passphrase
    let sym_key = crypto::derive_key(&args.passphrase);

    // Bind UDP socket
    let socket = net::bind_socket().await;

    // Set up event channel
    let (tx, mut rx) = mpsc::channel::<IncomingMessage>(256);

    // Spawn recv loop
    let recv_socket = Arc::clone(&socket);
    tokio::spawn(net::recv_loop(recv_socket, sym_key, own_pubkey, tx));

    // Spawn announce loop
    let announce_socket = Arc::clone(&socket);
    let announce_nick = args.nick.clone();
    let announce_key = signing_key.clone();
    tokio::spawn(async move {
        loop {
            let msg = Message::Announce {
                nickname: announce_nick.clone(),
            };
            net::send_message(&announce_socket, &sym_key, &announce_key, &msg).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    // Send initial announce
    let msg = Message::Announce {
        nickname: args.nick.clone(),
    };
    net::send_message(&socket, &sym_key, &signing_key, &msg).await;

    // Initialize UI
    let mut ui = Ui::new();
    let mut peers = PeerState::default();
    let history_path = data.join("history.jsonl");

    // Load history
    let history_entries = history::load_recent(&history_path, args.history);
    if !history_entries.is_empty() {
        ui.push_system(&format!("── loaded {} history messages ──", history_entries.len()));
        for entry in &history_entries {
            let ts = chrono::DateTime::from_timestamp(entry.timestamp, 0)
                .map(|dt| dt.format("%H:%M").to_string())
                .unwrap_or_default();
            ui.push_line(
                &format!("[{ts}] {}", entry.nickname),
                crossterm::style::Color::DarkGrey,
                &entry.text,
            );
        }
        ui.push_system("── end of history ──");
    }

    ui.push_system(&format!(
        "You are \"{}\". Type a message and press Enter. Ctrl+C to quit.",
        args.nick
    ));
    ui.render();

    // Main input loop
    loop {
        // Check for incoming network messages (non-blocking drain)
        while let Ok(incoming) = rx.try_recv() {
            handle_incoming(&mut ui, &mut peers, &history_path, incoming);
        }

        // Poll for keystrokes (50ms timeout so we keep checking network)
        if let Some(key) = ui.poll_key(Duration::from_millis(50)) {
            if let Some(line) = ui.handle_key(key) {
                if line == "\x03" {
                    // Ctrl+C — send Leave and exit
                    let msg = Message::Leave {
                        nickname: args.nick.clone(),
                    };
                    net::send_message(&socket, &sym_key, &signing_key, &msg).await;
                    break;
                }

                // Validate length
                if line.len() > MAX_TEXT_LEN {
                    ui.push_system(&format!(
                        "Message too long ({} bytes, max {MAX_TEXT_LEN})",
                        line.len()
                    ));
                    continue;
                }

                let now = Utc::now().timestamp();
                let ts = Utc::now().format("%H:%M").to_string();

                // Send chat message
                let msg = Message::Chat {
                    nickname: args.nick.clone(),
                    text: line.clone(),
                    timestamp: now,
                };
                net::send_message(&socket, &sym_key, &signing_key, &msg).await;

                // Display own message
                let own_color = peers.color_for(&own_pubkey);
                ui.push_line(&format!("[{ts}] {}", args.nick), own_color, &line);

                // Save to history
                history::append(
                    &history_path,
                    &HistoryEntry {
                        timestamp: now,
                        nickname: args.nick.clone(),
                        text: line,
                    },
                );
            }
        }
    }

    // Cleanup handled by Ui::drop
}

fn handle_incoming(
    ui: &mut Ui,
    peers: &mut PeerState,
    history_path: &std::path::Path,
    incoming: IncomingMessage,
) {
    let color = peers.color_for(&incoming.sender_pubkey);

    match incoming.message {
        Message::Announce { ref nickname } => {
            let is_new = peers.upsert(incoming.sender_pubkey, nickname);
            if is_new {
                ui.push_system(&format!("{nickname} joined the chat"));
                ui.bell();
            }
        }
        Message::Chat {
            ref nickname,
            ref text,
            timestamp,
        } => {
            peers.upsert(incoming.sender_pubkey, nickname);
            let ts = chrono::DateTime::from_timestamp(timestamp, 0)
                .map(|dt| dt.format("%H:%M").to_string())
                .unwrap_or_default();
            ui.push_line(&format!("[{ts}] {nickname}"), color, text);
            ui.bell();

            history::append(
                history_path,
                &HistoryEntry {
                    timestamp,
                    nickname: nickname.clone(),
                    text: text.clone(),
                },
            );
        }
        Message::Leave { ref nickname } => {
            peers.remove(&incoming.sender_pubkey);
            ui.push_system(&format!("{nickname} left the chat"));
            ui.bell();
        }
    }
}
