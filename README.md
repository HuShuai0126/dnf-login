# dnf-login

A launcher for Dungeon & Fighter (DNF) written in Rust.


## Features

### Server

- Account login, registration, password change, and password recovery
- Input validation on all fields before any database access
- Per-IP rate limiting (10 requests per 60-second window)
- Encrypted communication: all request and response bodies are AES-256-GCM ciphertext

### Client

- Multi-language support: English, Simplified Chinese, Traditional Chinese, Japanese, Korean
- Optional credential storage encrypted with Windows DPAPI
- Custom wallpaper directory with five fill modes: Tile, Stretch, Fill, Center, Fit
- Detects whether the game is already running before launching


## Building

**Prerequisites:**

- [Rust](https://rustup.rs/) toolchain (stable channel)

**Build commands:**

```bash
# Build all crates
cargo build --release

# Build a specific crate
cargo build --release -p dnf-server
cargo build --release -p dnf-client

# Run tests
cargo test
```

The client crate is Windows-only. Cross-compiling from Linux:

```bash
cargo build --release -p dnf-client --target x86_64-pc-windows-gnu
```


## Server Deployment

**Requirements:**

- Linux host
- MySQL 5.x with an existing DNF database (`d_taiwan` and related schemas)
- RSA private key matching the game's public key

**Setup:**

1. Copy `server/.env.example` to `server/.env` and fill in the values.
2. Set `RSA_PRIVATE_KEY_PATH` in `server/.env` to the path of the RSA private key file.
3. Start the server:

```bash
cd server && cargo run --release
```

**Configuration variables:**

| Variable | Required | Default | Notes |
|---|---|---|---|
| `AES_KEY` | yes | | 64 hex characters (32 bytes). Generate with `openssl rand -hex 32`. Must match the client. |
| `DB_PASSWORD` | yes | | Plain text. Special characters do not need escaping. |
| `DB_HOST` | no | `127.0.0.1` | |
| `DB_PORT` | no | `3306` | |
| `DB_USER` | no | `game` | |
| `DB_NAME` | no | `d_taiwan` | |
| `RSA_PRIVATE_KEY_PATH` | no | `/data/privatekey.pem` | |
| `BIND_ADDRESS` | no | `0.0.0.0:5505` | |
| `INITIAL_CERA` | no | `1000` | |
| `INITIAL_CERA_POINT` | no | `0` | |
| `RUST_LOG` | no | `info` | e.g. `info,dnf_gate_server=debug` |


## Client Deployment

1. Place the launcher executable in the game directory alongside `DNF.exe`.
2. Copy `Config.example.toml` to `Config.toml` in the same directory and set `server_url` and `aes_key`. These can also be configured from the in-app settings screen.

```toml
server_url = "http://192.168.200.131:5505"
aes_key    = "<64 hex characters matching the server>"
```


## License

MIT License - see the [LICENSE](LICENSE) file for details.
