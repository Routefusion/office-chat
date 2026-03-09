use std::fs;
use std::path::Path;

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;

/// Load or generate an Ed25519 keypair from `~/.office-chat/keypair.bin`.
pub fn load_or_generate_keypair(path: &Path) -> SigningKey {
    if path.exists() {
        let bytes = fs::read(path).expect("failed to read keypair file");
        let secret: [u8; 32] = bytes.try_into().expect("invalid keypair file length");
        SigningKey::from_bytes(&secret)
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create config directory");
        }
        let key = SigningKey::generate(&mut OsRng);
        fs::write(path, key.to_bytes()).expect("failed to write keypair file");
        key
    }
}

/// Derive a 32-byte symmetric key from a passphrase using Argon2id.
pub fn derive_key(passphrase: &str) -> [u8; 32] {
    let salt = b"office-chat-salt"; // fixed salt — key is per-channel, not per-user
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .expect("argon2 key derivation failed");
    key
}

/// Encrypt plaintext with ChaCha20-Poly1305, returning (ciphertext, nonce).
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> (Vec<u8>, [u8; 12]) {
    let cipher = ChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).expect("encryption failed");
    (ciphertext, nonce_bytes)
}

/// Decrypt ciphertext with ChaCha20-Poly1305.
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8; 12]) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce);
    cipher.decrypt(nonce, ciphertext).ok()
}

/// Sign a message with the signing key.
pub fn sign(key: &SigningKey, data: &[u8]) -> Vec<u8> {
    key.sign(data).to_bytes().to_vec()
}

/// Verify a signature against a public key.
pub fn verify(pubkey: &[u8; 32], data: &[u8], signature: &[u8]) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(pubkey) else {
        return false;
    };
    let sig_bytes: [u8; 64] = match signature.try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify(data, &sig).is_ok()
}
