# P2

**Your team. Your community. Your game.**

P2 is a self-hosted desktop platform for communication and collaboration. It brings channels, direct and group messaging, voice and video calls, screen sharing, task boards, documents and a shared game library together in a single application — running on your own server, under your own rules.

P2 is built for game studios, but not only for them: friend groups, player communities, clubs and any team that wants to keep its data on its own infrastructure can use it.

- Website: <https://p2.tdagaming.net>
- Documentation: <https://p2.tdagaming.net/docs/>
- Privacy policy: <https://p2.tdagaming.net/docs/privacy/>

## Download

| Platform | Package |
|---|---|
| Windows 10 / 11 | P2 Client on the Microsoft Store ("Play Through") |
| macOS | P2 Client — see the [latest release](../../releases/latest) |
| Windows server | `P2-Server-Setup-<version>.exe` — see the [latest release](../../releases/latest) |
| Ubuntu 24.04+ server | `P2-Server-Ubuntu-24.04-x64-<version>.tar.gz` — see the [latest release](../../releases/latest) |

iOS and Android clients are on the way.

## Features

- **Messaging** — project and community channels, direct and group messages, replies, quotes, reactions and animated profile pictures
- **Voice, video and screen sharing** — calls with camera and screen sharing up to 4K; viewers choose their own quality and can rewind a live stream
- **Live annotation** — participants draw on a shared screen and everyone sees it in real time
- **In-game overlay** (Windows) — chat, voice room, browser and tasks on top of the game
- **Work** — organizations, roles, user groups and per-channel permissions; Kanban boards, sprints, tasks, notes, documents and a freeform canvas
- **Game library** — install, update and launch games from one place; teams publish builds and players download only what changed
- **Discover and server browser** — find public communities and join in one click; browse every P2 Server that chooses to be listed, public or invite-only
- **Self-hosted** — PostgreSQL-backed server for Windows and Ubuntu, one-file `.p2backup` backups, Windows/Linux services

## Architecture

P2 is written entirely in Rust. The desktop client and the server console use a native, GPU-rendered UI (GPUI) and ship no browser engine. The server runs on axum + sqlx + PostgreSQL.

The repository root is a Cargo workspace; the single source of truth for the product version is `[workspace.package] version` in `Cargo.toml`.

| Path | Contents |
|---|---|
| `apps/p2-client` | Desktop client (Windows, macOS): onboarding, workspace, chat, DMs and groups, calls, task boards, documents, library, admin panel, settings, tray, updater |
| `apps/p2-server` | API server (axum + sqlx): REST, real-time events, call signalling, release storage, update channels, server directory |
| `apps/p2-server-console` | P2 Server Console (Windows): database setup, start/stop, Windows service, backup/restore, updates |
| `apps/p2-overlay` | In-game overlay host: off-screen GPUI panels, notifications, HUD, tool windows |
| `apps/p2-overlay-host`, `apps/p2-d3dtest` | Overlay development and test tools |
| `apps/p2-callbot` | Call participant for testing (WebRTC, no audio device) |
| `apps/p2-wincap` | Test tool: captures a process window to PNG |
| `apps/p2-showcase` | UI component showcase and measurements |
| `crates/p2-client-core` | API client, session storage, local settings, real-time connection |
| `crates/p2-ui` | Design system: tokens, themes, motion, shared components |
| `crates/p2-i18n` | 11 languages (`locales/*.json`) |
| `crates/p2-media` | Call engine: WebRTC mesh, microphone/speaker, audio processing, screen and camera capture |
| `crates/p2-patcher` | Library engine: content-defined chunking, install/update/verify/repair, launching, build upload |
| `crates/p2-overlay-client`, `crates/p2-overlay-ipc` | Client-side overlay manager and the shared-memory contract with the overlay DLL |
| `crates/p2-server-ops` | Server operations: configuration, logs, migrations, Windows service, `.p2backup` |
| `crates/p2-updater` | Updater: feed, download, sha512 verification, silent install |
| `native/overlay` | C++ DLL injected into games, plus the injector (CMake/MSVC) |
| `packaging/` | Windows installers, MSIX, macOS packaging, Linux server package |
| `deploy/` | Linux install scripts, systemd unit, `p2-serverctl`, Windows service files |
| `tools/` | End-to-end tests and helper scripts |
| `third_party/` | Locally patched GPUI dependencies |

## Requirements

- Rust stable (MSVC toolchain on Windows): `winget install Rustlang.Rustup`
- Windows 10/11 x64 with the Visual Studio C++ build tools (MSVC + Windows SDK; CMake for the overlay DLL) for the client, server console and installers
- macOS with Xcode command line tools for the macOS client (`packaging/build-macos.sh`)
- PostgreSQL 15+ for the server (the server runs on Windows and Linux)
- Node.js 22+ for packaging and end-to-end tests (built-in modules only, no `npm install`)

## Build and run

```bash
cargo build --release                  # whole workspace
cargo run --release -p p2-client       # desktop client
cargo run --release -p p2-server-console

# Server (PostgreSQL with the P2 schema; migrations: p2-server migrate)
DATABASE_URL=postgresql://... JWT_ACCESS_SECRET=... JWT_REFRESH_SECRET=... PORT=4000 \
  cargo run --release -p p2-server

# Unit tests (no audio device is opened)
P2_NO_AUDIO_DEVICE=1 cargo test --release --workspace
```

## Self-hosting

- **Ubuntu:** extract the package and run `sudo bash install.sh` (fresh install or upgrade). Details: [docs/INSTALL-LINUX.md](docs/INSTALL-LINUX.md)
- **Windows:** run `P2-Server-Setup-<version>.exe`, then open P2 Server Console. Details: [docs/INSTALL-WINDOWS.md](docs/INSTALL-WINDOWS.md)

```bash
tar -xzf P2-Server-Ubuntu-24.04-x64-<version>.tar.gz
cd P2-Server-Ubuntu-24.04-x64-<version>
sudo bash install.sh
sudo p2-serverctl status
```

Run P2 Server behind an HTTPS reverse proxy instead of exposing port `4000` directly, and never expose PostgreSQL (`5432`) to the internet. Production notes: [docs/SECURITY.md](docs/SECURITY.md).

**Backups:** the **Back up** button in Windows Server Console, or `sudo p2-serverctl backup` on Ubuntu, collects the database and uploaded files into a single `.p2backup` file (`sudo p2-serverctl restore <file>` restores it). Game builds can be large, so including them is optional.

Prefer not to run a server yourself? Managed hosting is available at <https://p2.tdagaming.net>.

## Contributing

Issues and pull requests are welcome. Please describe the problem or change clearly and include steps to reproduce for bug reports. Some documents under `docs/` are currently written in Turkish.

## License

P2 is released under the [MIT License](LICENSE). Third-party components ship under their own licenses: `third_party/` (GPUI patches, Apache-2.0), `crates/p2-ui/assets/fonts` (Inter, SIL OFL), `packaging/vendor/` (NSIS, WinSW, 7-Zip; license files are in their folders).
