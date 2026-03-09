# office-chat

Encrypted LAN chat over UDP broadcast. Zero infrastructure — just a shared passphrase and a local network.

```
┌──────────────────────────────────────────┐
│ [14:32] alice  hey, anyone around?       │
│ [14:32] bob    yeah what's up            │
│ [14:33] alice  deploy going out at 3     │
│ *** carol joined the chat                │
│ [14:33] carol  i'm here too              │
│──────────────────────────────────────────│
│ > _                                      │
└──────────────────────────────────────────┘
```

## Install

### Homebrew

```bash
brew tap routefusion/office-chat
brew install office-chat
```

### From source

```bash
git clone https://github.com/routefusion/office-chat.git
cd office-chat
cargo build --release
cp target/release/office-chat /usr/local/bin/
```

## Usage

```bash
office-chat --nick alice --passphrase "correct horse battery staple"
```

On another machine (same LAN):

```bash
office-chat --nick bob --passphrase "correct horse battery staple"
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--nick`, `-n` | required | Your display name |
| `--passphrase`, `-p` | required | Shared secret for encryption |
| `--history` | `50` | Number of history messages to load on startup |

### Controls

- **Enter** — send message
- **Ctrl+C** — leave chat and exit

## How it works

### Encryption

1. Shared passphrase → **Argon2id** → 32-byte symmetric key
2. Each message encrypted with **ChaCha20-Poly1305** AEAD
3. Each user has an **Ed25519** keypair (generated on first run) — every message is signed to prevent spoofing

One encrypted packet per message, not N copies. Practical for broadcast UDP.

### Transport

- UDP broadcast to `255.255.255.255:7337`
- `SO_BROADCAST` + `SO_REUSEADDR` + `SO_REUSEPORT` (macOS)
- Self-echo filtered by comparing sender public key

### Wire format

```
Packet { ciphertext, nonce[12], signature[64], sender_pubkey[32] }
  └─ decrypts to → Message::Announce { nickname }
                  | Message::Chat { nickname, text, timestamp }
                  | Message::Leave { nickname }
```

Serialized with bincode. Max message text: 1000 bytes (stays under 1472-byte UDP/Ethernet MTU).

### Terminal UI

- crossterm raw mode, no TUI framework
- Scrolling message area + fixed input line at bottom
- Deterministic nickname colors from public key
- Terminal bell (`BEL`) on incoming messages

## Data directory

Stored in `~/.office-chat/`:

| File | Purpose |
|------|---------|
| `keypair.bin` | Ed25519 signing key (generated on first run) |
| `history.jsonl` | Local message history (append-only, unencrypted) |

## Architecture

```
main()
  ├── spawn: recv_loop       → reads UDP → mpsc::Sender<Event>
  ├── spawn: announce_loop   → sends Announce every 30s
  └── run:   input_loop      → polls keystrokes + incoming events
                                (only task that writes to stdout)
```

## Project structure

```
src/
├── main.rs       # CLI args, tokio runtime, task orchestration
├── crypto.rs     # Keypair load/gen, Argon2 KDF, encrypt/decrypt, sign/verify
├── protocol.rs   # Packet + Message types, bincode serialization
├── net.rs        # UDP socket setup, send/recv async loops
├── state.rs      # Peer tracking, color assignment
├── history.rs    # JSONL append + load-recent-N
└── ui.rs         # crossterm rendering, keystroke handling, input buffer
```

## Releasing

```bash
git tag v0.1.0 && git push --tags
cargo build --release
# Create GitHub release, attach target/release/office-chat
# Update Formula/office-chat.rb with new URL + SHA256
```
