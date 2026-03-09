use serde::{Deserialize, Serialize};

/// Maximum text length in bytes (keeps UDP packets under 1472-byte Ethernet MTU).
pub const MAX_TEXT_LEN: usize = 1000;

/// Cleartext message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Announce { nickname: String },
    Chat { nickname: String, text: String, timestamp: i64 },
    Leave { nickname: String },
}

/// Wire packet: encrypted message + crypto metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
    /// Ed25519 signature stored as Vec<u8> (64 bytes) for serde compatibility.
    pub signature: Vec<u8>,
    pub sender_pubkey: [u8; 32],
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).expect("packet serialization failed")
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        bincode::deserialize(data).ok()
    }
}
