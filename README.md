# office-chat

LAN chat over UDP broadcast. Zero infrastructure, zero config — just run it and talk to anyone on the same network.

```
┌──────────────────────────────────────────────────────────────────┐
│ *** Doomed Crungle the Regrettable joined the chat              │
│ [14:32] Baron Kragmire Eater of Bees  hey, anyone around?       │
│ [14:32] Prophet Toadmire the Shameless  yeah what's up          │
│ [14:33] Baron Kragmire Eater of Bees  deploy going out at 3    │
│ [14:33] Doomed Crungle the Regrettable  i'm here too            │
│─────────────────────────────────────────────────────────────────│
│ > _                                                              │
└──────────────────────────────────────────────────────────────────┘
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
cargo install --path .
```

## Usage

```bash
office-chat
```

That's it. You get a random name and you're in. Every session you're someone new.

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--history` | `50` | Number of history messages to load on startup |

### Controls

- **Enter** — send message
- **Ctrl+C** — leave chat and exit

## How it works

### Signing & Encoding

- Each user has an **Ed25519** keypair (generated on first run) — every message is signed to prevent spoofing
- Messages are encoded with **ChaCha20-Poly1305** using a fixed key (prevents casual packet sniffing, not a security boundary)
- The real access control is your LAN — if you're on the network, you're in

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
| `history.jsonl` | Local message history (append-only) |

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
