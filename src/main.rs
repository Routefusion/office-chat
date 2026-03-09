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
use crate::lore::Lore;
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
}

fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home directory")
        .join(".office-chat")
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

#[tokio::main]
async fn main() {
    let args = Args::parse();
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

    // Lore system
    let mut lore = Lore::new();
    let mut lore_timer = tokio::time::interval(Duration::from_secs(lore.next_delay_secs()));
    // Skip the first immediate tick
    lore_timer.tick().await;

    // Main loop: select between network messages, terminal events, and lore
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
                            // Handle lore commands
                            if line == "/fight" {
                                if let Some(result) = lore.handle_fight() {
                                    ui.push_system(&result);
                                } else {
                                    ui.push_system("There is nothing to fight. For now.");
                                }
                                continue;
                            }
                            if line == "/flee" {
                                if let Some(result) = lore.handle_flee() {
                                    ui.push_system(&result);
                                } else {
                                    ui.push_system("You flee from nothing. Cowardice noted.");
                                }
                                continue;
                            }

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
            _ = lore_timer.tick() => {
                let peer_nicks = peers.nicknames();
                // 25% chance of encounter, 75% passive event
                let mut rng = rand::thread_rng();
                if rng.gen_bool(0.25) {
                    let msg = lore.spawn_encounter();
                    ui.push_system(&msg);
                } else {
                    let event = lore.random_event(&peer_nicks);
                    ui.push_system(&event);
                }
                // Randomize next interval
                lore_timer = tokio::time::interval(Duration::from_secs(lore.next_delay_secs()));
                lore_timer.tick().await; // skip immediate tick
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
