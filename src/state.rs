use std::collections::HashMap;

use crossterm::style::Color;

/// Info about a known peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub nickname: String,
    pub color: Color,
}

/// Map of known peers keyed by their public key.
#[derive(Debug, Default)]
pub struct PeerState {
    pub peers: HashMap<[u8; 32], PeerInfo>,
}

/// Deterministic color from the first byte of the public key.
fn color_for_pubkey(pubkey: &[u8; 32]) -> Color {
    const COLORS: [Color; 7] = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::White,
    ];
    COLORS[(pubkey[0] as usize) % COLORS.len()]
}

impl PeerState {
    pub fn upsert(&mut self, pubkey: [u8; 32], nickname: &str) -> bool {
        let is_new = !self.peers.contains_key(&pubkey);
        self.peers.entry(pubkey).or_insert_with(|| PeerInfo {
            nickname: nickname.to_string(),
            color: color_for_pubkey(&pubkey),
        });
        if !is_new {
            if let Some(info) = self.peers.get_mut(&pubkey) {
                info.nickname = nickname.to_string();
            }
        }
        is_new
    }

    pub fn remove(&mut self, pubkey: &[u8; 32]) {
        self.peers.remove(pubkey);
    }

    pub fn nicknames(&self) -> Vec<String> {
        self.peers.values().map(|p| p.nickname.clone()).collect()
    }

    pub fn color_for(&self, pubkey: &[u8; 32]) -> Color {
        self.peers
            .get(pubkey)
            .map(|p| p.color)
            .unwrap_or_else(|| color_for_pubkey(pubkey))
    }
}
