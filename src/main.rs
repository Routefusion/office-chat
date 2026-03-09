mod ai;
mod crypto;
mod history;
mod lore;
mod net;
mod protocol;
mod state;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use rand::Rng;
use ed25519_dalek::SigningKey;
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::history::HistoryEntry;
use crate::net::IncomingMessage;
use crate::protocol::{Message, MAX_TEXT_LEN};
use crate::state::PeerState;
use crate::ui::Ui;

/// Fixed key for LAN encryption — not a secret, just prevents casual sniffing.
/// Real security boundary is being on the LAN.
const CHANNEL_KEY: &str = "office-chat-lan-channel";

#[derive(Parser)]
#[command(name = "office-chat", about = "LAN chat over UDP broadcast")]
struct Args {
    /// Number of history messages to load on startup
    #[arg(long, default_value = "50")]
    history: usize,

    /// Update to the latest release from GitHub
    #[arg(long)]
    update: bool,

    /// Run as the AI Loremaster (headless, requires Ollama)
    #[arg(long)]
    loremaster: bool,
}

fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home directory")
        .join(".office-chat")
}

const RELEASE_URL: &str =
    "https://github.com/Routefusion/office-chat/releases/latest/download/office-chat";

fn self_update() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let current_exe = std::env::current_exe().expect("cannot determine current executable path");

    println!("Downloading latest release...");

    let tmp = current_exe.with_extension("tmp");

    let status = Command::new("curl")
        .args(["-fSL", RELEASE_URL, "-o"])
        .arg(&tmp)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let _ = fs::remove_file(&tmp);
            eprintln!("Download failed (exit {}).", s.code().unwrap_or(-1));
            std::process::exit(1);
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            eprintln!("Failed to run curl: {e}");
            std::process::exit(1);
        }
    }

    // Make executable
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
        .expect("failed to set permissions");

    // Replace current binary
    fs::rename(&tmp, &current_exe).expect("failed to replace binary");

    println!("Updated! Restart office-chat to use the new version.");
}

/// Generate a chaotic lore nickname.
fn random_nick() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    const TITLES: &[&str] = &[
        "Sir", "Lord", "Saint", "Baron", "Duke", "Count", "Keeper",
        "Warden", "Prophet", "Herald", "Archon", "Seraph", "Deacon",
        "Regent", "Vizier", "Abbot", "Thane", "Consul", "Paladin",
        "Scribe", "Witch", "Sage", "Elder", "Chosen", "Forsaken",
        "Blessed", "Cursed", "Mad", "Dread", "Feral", "Hollow",
        "Exalted", "Fallen", "Pale", "Iron", "Void", "Storm",
        "Blood", "Ghost", "Bone", "Ash", "Grim", "Half-Dead",
        "Twice-Banished", "Unchained", "Oathless", "Doomed",
    ];

    const NAMES: &[&str] = &[
        "Morbius", "Gorthak", "Xylphren", "Bingleton", "Chadwick",
        "Eldreth", "Fumblor", "Grimbald", "Hextooth", "Inkfang",
        "Jorbulus", "Kragmire", "Lungsworth", "Mungus", "Norbington",
        "Oggrick", "Plimbus", "Quagsworth", "Rotgut", "Skumble",
        "Throckmorton", "Ulgreth", "Vexmoor", "Wormald", "Xanthippos",
        "Yorbel", "Zymurgo", "Blightwick", "Crungle", "Dankworth",
        "Festerling", "Gnarlbone", "Humgrove", "Irontaint", "Jibbles",
        "Krumhorn", "Lumpkin", "Moldric", "Nubsworth", "Orkblat",
        "Pusgrave", "Quimley", "Ratsworth", "Splotch", "Toadmire",
        "Ungus", "Vilehorn", "Wretchard", "Xogbog", "Yeastwick",
        "Zurgle", "Grimshaw", "Bonechill", "Corpseflower", "Doomhollow",
        "Fleshwick", "Gobbsworth", "Hagraven", "Ironbelly", "Jagsworth",
        "Knottbeard", "Lichfield", "Mossrot", "Nightsoil", "Oggsworth",
        "Plagueborn", "Quagmort", "Rottingham", "Skulkgrove", "Tombald",
    ];

    const SUFFIXES: &[&str] = &[
        "the Unhinged", "the Damp", "the Inconsolable", "the Girthy",
        "the Befouled", "the Regrettable", "the Unwiped", "the Eternal",
        "the Moist", "the Questionable", "the Unbothered", "the Putrid",
        "the Forgotten", "the Soggy", "the Malodorous", "the Shameless",
        "the Unfortunate", "of the Swamp", "of the Crypt", "of the Mire",
        "the Flatulent", "the Incomprehensible", "the Banned",
        "the Unlicensed", "the Inevitable", "the Unnecessary",
        "of Dubious Origin", "the Oozing", "the Unwashed", "the Foul",
        "the Thrice-Divorced", "the Chafed", "the Bloated",
        "the Unsanctioned", "the Festering", "the Pungent",
        "Eater of Bees", "Who Dwells Below", "the Perspiring",
        "the Underpaid", "the Overthinker", "the Mouth-Breather",
        "the Last of His Name", "the First of Her Crimes",
        "the Surprisingly Agile", "the Deeply Confused",
        "Who Must Not Be Microwaved", "the Sentient", "the Fleshy",
    ];

    let title = TITLES[rng.gen_range(0..TITLES.len())];
    let name = NAMES[rng.gen_range(0..NAMES.len())];
    let suffix = SUFFIXES[rng.gen_range(0..SUFFIXES.len())];

    format!("{title} {name} {suffix}")
}

const LOREMASTER_NICK: &str = "The Loremaster";

/// Run as a headless AI Loremaster — no TUI, broadcasts responses to the LAN.
async fn run_loremaster() {
    use crate::lore::Lore;
    let data = data_dir();
    let signing_key = crypto::load_or_generate_keypair(&data.join("loremaster-keypair.bin"));
    let sym_key = crypto::derive_key(CHANNEL_KEY);
    let socket = net::bind_socket().await;
    let own_pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();

    // Network recv channel
    let (tx, mut rx) = mpsc::channel::<IncomingMessage>(256);
    let recv_socket = Arc::clone(&socket);
    tokio::spawn(net::recv_loop(recv_socket, sym_key, own_pubkey, tx));

    // Announce loop
    let announce_socket = Arc::clone(&socket);
    let announce_key = signing_key.clone();
    tokio::spawn(async move {
        loop {
            let msg = Message::Announce {
                nickname: LOREMASTER_NICK.to_string(),
            };
            net::send_message(&announce_socket, &sym_key, &announce_key, &msg).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    // Initial announce
    let msg = Message::Announce {
        nickname: LOREMASTER_NICK.to_string(),
    };
    net::send_message(&socket, &sym_key, &signing_key, &msg).await;

    // AI channel
    let (ai_response_tx, mut ai_response_rx) = mpsc::channel::<String>(32);
    let ai_tx = ai::spawn(ai_response_tx);

    // Lore system for periodic events
    let mut lore = Lore::new();
    let mut peers = PeerState::default();
    let mut lore_timer = tokio::time::interval(Duration::from_secs(lore.next_delay_secs()));
    lore_timer.tick().await;

    eprintln!("Loremaster is watching the LAN. Ctrl+C to banish.");

    loop {
        tokio::select! {
            Some(incoming) = rx.recv() => {
                match incoming.message {
                    Message::Announce { ref nickname } => {
                        let is_new = peers.upsert(incoming.sender_pubkey, nickname);
                        if is_new {
                            eprintln!("[+] {nickname} joined");
                        }
                    }
                    Message::Chat { ref nickname, ref text, .. } => {
                        peers.upsert(incoming.sender_pubkey, nickname);
                        eprintln!("[chat] {nickname}: {text}");

                        // Handle commands
                        if let Some(question) = text.strip_prefix("/ask ") {
                            let question = question.trim();
                            if !question.is_empty() {
                                let _ = ai_tx.send(ai::AiRequest::Ask {
                                    user_nick: nickname.clone(),
                                    question: question.to_string(),
                                }).await;
                            }
                        } else if text == "/fight" {
                            if let Some(result) = lore.handle_fight(nickname) {
                                let now = Utc::now().timestamp();
                                let msg = Message::Chat {
                                    nickname: LOREMASTER_NICK.to_string(),
                                    text: result.clone(),
                                    timestamp: now,
                                };
                                net::send_message(&socket, &sym_key, &signing_key, &msg).await;
                                eprintln!("[lore] {result}");
                            }
                        } else if text == "/flee" {
                            if let Some(result) = lore.handle_flee(nickname) {
                                let now = Utc::now().timestamp();
                                let msg = Message::Chat {
                                    nickname: LOREMASTER_NICK.to_string(),
                                    text: result.clone(),
                                    timestamp: now,
                                };
                                net::send_message(&socket, &sym_key, &signing_key, &msg).await;
                                eprintln!("[lore] {result}");
                            }
                        } else {
                            // Always respond if addressed, 15% chance otherwise
                            let addressed = text.to_lowercase().contains("loremaster");
                            if addressed || rand::thread_rng().gen_bool(0.15) {
                                let _ = ai_tx.send(ai::AiRequest::ChatMessage {
                                    nickname: nickname.clone(),
                                    text: text.clone(),
                                }).await;
                            }
                        }
                    }
                    Message::Leave { ref nickname } => {
                        peers.remove(&incoming.sender_pubkey);
                        eprintln!("[-] {nickname} left");
                    }
                }
            }
            Some(response) = ai_response_rx.recv() => {
                eprintln!("[loremaster] {response}");
                let now = Utc::now().timestamp();
                let msg = Message::Chat {
                    nickname: LOREMASTER_NICK.to_string(),
                    text: response,
                    timestamp: now,
                };
                net::send_message(&socket, &sym_key, &signing_key, &msg).await;
            }
            _ = lore_timer.tick() => {
                let peer_nicks = peers.nicknames();
                let mut rng = rand::thread_rng();
                if rng.gen_bool(0.25) {
                    // Spawn encounter — broadcast as system-style message
                    let event = lore.spawn_encounter();
                    let now = Utc::now().timestamp();
                    let msg = Message::Chat {
                        nickname: LOREMASTER_NICK.to_string(),
                        text: event.clone(),
                        timestamp: now,
                    };
                    net::send_message(&socket, &sym_key, &signing_key, &msg).await;
                    eprintln!("[lore] {event}");
                } else {
                    let event = lore.random_event(&peer_nicks);
                    let now = Utc::now().timestamp();
                    let msg = Message::Chat {
                        nickname: LOREMASTER_NICK.to_string(),
                        text: event.clone(),
                        timestamp: now,
                    };
                    net::send_message(&socket, &sym_key, &signing_key, &msg).await;
                    eprintln!("[lore] {event}");
                    // 40% chance the AI also adds a comment
                    if rng.gen_bool(0.4) {
                        let _ = ai_tx.send(ai::AiRequest::LoreEvent {
                            event_text: event,
                        }).await;
                    }
                }
                lore_timer = tokio::time::interval(Duration::from_secs(lore.next_delay_secs()));
                lore_timer.tick().await;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.update {
        self_update();
        return;
    }

    if args.loremaster {
        run_loremaster().await;
        return;
    }

    let data = data_dir();

    // Generate a random lore nickname
    let nick = random_nick();

    // Load or generate signing keypair
    let signing_key: SigningKey = crypto::load_or_generate_keypair(&data.join("keypair.bin"));
    let own_pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();

    // Fixed symmetric key — anyone on the LAN can join
    let sym_key = crypto::derive_key(CHANNEL_KEY);

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

    // Async event stream for crossterm
    let mut term_events = EventStream::new();

    // Main loop: network messages and terminal events only
    loop {
        tokio::select! {
            Some(incoming) = rx.recv() => {
                handle_incoming(&mut ui, &mut peers, &history_path, incoming);
            }
            Some(Ok(event)) = term_events.next() => {
                match event {
                    Event::Key(key) => {
                        // Ctrl+C
                        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            let msg = Message::Leave { nickname: nick.clone() };
                            net::send_message(&socket, &sym_key, &signing_key, &msg).await;
                            break;
                        }

                        if let Some(line) = ui.handle_key(key) {
                            if line.len() > MAX_TEXT_LEN {
                                ui.push_system(&format!(
                                    "Message too long ({} bytes, max {MAX_TEXT_LEN})",
                                    line.len()
                                ));
                                continue;
                            }

                            let now = Utc::now().timestamp();
                            let ts = Utc::now().format("%H:%M").to_string();

                            let msg = Message::Chat {
                                nickname: nick.clone(),
                                text: line.clone(),
                                timestamp: now,
                            };
                            net::send_message(&socket, &sym_key, &signing_key, &msg).await;

                            let own_color = peers.color_for(&own_pubkey);
                            ui.push_line(&format!("[{ts}] {}", nick), own_color, &line);

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
                    Event::Resize(_, _) => {
                        ui.render();
                    }
                    _ => {}
                }
            }
        }
    }
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
