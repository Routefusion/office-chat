mod crypto;
mod history;
mod net;
mod protocol;
mod state;
mod ui;

use std::fs;
use std::io::{self, Write};
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
    /// Your display nickname (prompted if not provided)
    #[arg(short, long)]
    nick: Option<String>,

    /// Shared passphrase for encryption (auto-generated and saved if not provided)
    #[arg(short, long)]
    passphrase: Option<String>,

    /// Number of history messages to load on startup
    #[arg(long, default_value = "50")]
    history: usize,
}

fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home directory")
        .join(".office-chat")
}

/// Prompt the user for a nickname interactively.
fn prompt_nick() -> String {
    print!("Enter your nickname: ");
    io::stdout().flush().ok();
    let mut nick = String::new();
    io::stdin().read_line(&mut nick).expect("failed to read nickname");
    let nick = nick.trim().to_string();
    if nick.is_empty() {
        eprintln!("Nickname cannot be empty.");
        std::process::exit(1);
    }
    nick
}

/// Load or generate a random passphrase, persisted to `~/.office-chat/passphrase`.
fn load_or_generate_passphrase(data: &PathBuf) -> String {
    let path = data.join("passphrase");
    if path.exists() {
        let phrase = fs::read_to_string(&path).expect("failed to read passphrase file");
        let phrase = phrase.trim().to_string();
        if !phrase.is_empty() {
            return phrase;
        }
    }
    // Generate a random passphrase: 4 words from a small wordlist
    let words: &[&str] = &[
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
        "india", "juliet", "kilo", "lima", "mike", "november", "oscar", "papa",
        "quebec", "romeo", "sierra", "tango", "uniform", "victor", "whiskey", "xray",
        "yankee", "zulu", "anchor", "barrel", "castle", "dagger", "falcon", "garden",
        "hammer", "island", "jungle", "kettle", "lantern", "marble", "needle", "oracle",
        "parrot", "quartz", "rocket", "saddle", "timber", "umbrella", "velvet", "walrus",
        "cobalt", "drift", "ember", "flint", "grove", "hatch", "ivory", "jade",
        "karma", "latch", "mesa", "nimbus", "opal", "plume", "ridge", "spark",
    ];
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let phrase: Vec<&str> = (0..4).map(|_| words[rng.gen_range(0..words.len())]).collect();
    let phrase = phrase.join("-");
    fs::create_dir_all(data).ok();
    fs::write(&path, &phrase).expect("failed to write passphrase file");
    phrase
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let data = data_dir();

    // Resolve nickname: flag > interactive prompt
    let nick = args.nick.unwrap_or_else(prompt_nick);

    // Resolve passphrase: flag > saved file > generate new
    let passphrase = args.passphrase.unwrap_or_else(|| {
        let phrase = load_or_generate_passphrase(&data);
        println!("Using passphrase: {phrase}");
        println!("Share this with others so they can join the same channel.\n");
        phrase
    });

    // Load or generate signing keypair
    let signing_key: SigningKey = crypto::load_or_generate_keypair(&data.join("keypair.bin"));
    let own_pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();

    // Derive symmetric key from passphrase
    let sym_key = crypto::derive_key(&passphrase);

    // Bind UDP socket
    let socket = net::bind_socket().await;

    // Set up event channel
    let (tx, mut rx) = mpsc::channel::<IncomingMessage>(256);

    // Spawn recv loop
    let recv_socket = Arc::clone(&socket);
    tokio::spawn(net::recv_loop(recv_socket, sym_key, own_pubkey, tx));

    // Spawn announce loop
    let announce_socket = Arc::clone(&socket);
    let announce_nick = nick.clone();
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
        nickname: nick.clone(),
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
        nick
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
                        nickname: nick.clone(),
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
                    nickname: nick.clone(),
                    text: line.clone(),
                    timestamp: now,
                };
                net::send_message(&socket, &sym_key, &signing_key, &msg).await;

                // Display own message
                let own_color = peers.color_for(&own_pubkey);
                ui.push_line(&format!("[{ts}] {}", nick), own_color, &line);

                // Save to history
                history::append(
                    &history_path,
                    &HistoryEntry {
                        timestamp: now,
                        nickname: nick.clone(),
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
