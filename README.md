<div align="center">
  <img src="docs/images/cirrus-icon.png" width="88" alt="Cirrus iCloud desktop client logo" />
  <h1>Cirrus</h1>

  <p><strong>Open-source iCloud Photos downloader and Find My devices client for Windows, macOS, and Linux.</strong></p>
  <p>Browse iCloud Photos, download originals, and locate Apple devices from one native desktop app.</p>

  <p>
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-4B5563" alt="Windows, macOS, and Linux" />
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/Rust-1.88%2B-000000?logo=rust" alt="Rust" />
    <img src="https://img.shields.io/badge/Vue-3-4FC08D?logo=vuedotjs&logoColor=white" alt="Vue 3" />
    <img src="https://img.shields.io/badge/License-Apache--2.0-blue" alt="Apache 2.0" />
  </p>

  <p>English | <a href="README.zh-CN.md">简体中文</a></p>
</div>

![Cirrus iCloud Photos and Find My desktop client](docs/images/cirrus-preview.png)

## Why Cirrus

Cirrus is a local-first **iCloud desktop client for Windows, macOS, and Linux**. It combines two workflows that are awkward to use from a browser: downloading **iCloud Photos** and finding **Apple devices with Find My**.

- Native Tauri window with a loopback-only MCP bridge enabled by default
- Multi-tab workspace for Devices, Photos, Notes, Contacts, Calendar, and Settings
- One Apple ID + password + 2FA session for Photos and Find My
- Original photos and videos, including Live Photos
- Multi-account switching with cached avatars and account-scoped download tasks
- AES-256-GCM session encryption with the key stored in the OS credential store

## Features

### iCloud Photos

- Browse the iCloud Photo Library in a responsive grid
- Open photos and videos in a full-screen viewer
- Download original files instead of thumbnails
- Download by date range with account-scoped task history
- Configure download concurrency and bandwidth limits

### Find My devices

- View iPhone, iPad, Mac, AirPods, and other Apple devices
- Live map with location, accuracy, and last-seen time
- Battery, Activation Lock, and Lost Mode status
- Device artwork in the device list and a direct **Play Sound** action

### Desktop experience

- English and Simplified Chinese UI
- Light, dark, and system themes
- Multiple Apple accounts with session reuse
- Local SQLite storage for accounts, avatars, sessions, and download history

### MCP

- Optional stdio MCP bridge for AI clients and automation
- Client configuration tabs for Claude Code, Cursor, CodeBuddy Code, ZCode, TRAE, VS Code, Windsurf, Codex, DeepSeek Harness, OpenCode, Pi, Cherry Studio, and Qoder
- Per-account and per-tool MCP switches; accounts and tools are enabled by default and can be individually blocked
- 19 MCP tools covering login, account status, devices, Find My sound, photos, downloads, notes, contacts, calendar, and profile images
- Sign-in tool that asks for approval in Cirrus, then opens the native Cirrus sign-in window
- Apple ID, password, and two-factor codes stay inside Cirrus and are never returned to the MCP client
- Loopback-only bridge with a per-run random bearer token

## Install

Download the latest build for Windows, macOS, or Linux from [GitHub Releases](../../releases/latest), or build from source below.

Windows requires WebView2. Linux runtime support requires WebKitGTK 4.1 and a Secret Service implementation.

## Build from source

Requirements:

- Node.js 22.13+
- pnpm 10+
- Rust 1.88+
- Windows 10/11 with WebView2
- macOS with Xcode Command Line Tools
- Linux with WebKitGTK 4.1, an AppIndicator library, and Secret Service development packages

```sh
pnpm install
make dev       # start the desktop app in development mode
make package   # build the platform installers
```

For map rendering, copy `.env.example` to `.env.local` and set:

```env
VITE_AMAP_KEY=
VITE_AMAP_SECURITY_CODE=
```

## Usage

1. Sign in with your Apple ID and password.
2. Complete two-factor authentication.
3. Open **Devices** for Find My or **Photos** for the iCloud Photo Library.

To use MCP, open **Settings → MCP**, enable the bridge, choose the matching client tab to copy or write its config, and turn off any iCloud account that MCP must not use. MCP exposes 19 account, device, photo, download, note, contact, calendar, profile image, login, and logout tools.

The first sign-in requires 2FA. Later launches restore the encrypted session when it is still valid.

## Privacy and storage

Cirrus runs entirely on your machine and talks directly to Apple. There is no Cirrus account, cloud relay, or telemetry service. The MCP bridge listens only on `127.0.0.1`, is enabled by default, and requires a per-run bearer token.

Local data is stored in:

```text
Windows: %APPDATA%\com.cirrus\cirrus.db
macOS:   ~/Library/Application Support/com.cirrus/cirrus.db
Linux:   ~/.local/share/com.cirrus/cirrus.db
```

- Account metadata and download tasks live in SQLite tables.
- Cached avatars are stored locally.
- Session cookies and tokens are encrypted with AES-256-GCM before being written to SQLite.
- The encryption key is stored in Windows Credential Manager, macOS Keychain, or Linux Secret Service.
- Your password is never persisted.

## Tech stack

- **Desktop shell:** Tauri 2
- **Backend:** Rust
- **Frontend:** Vue 3, TypeScript, Tailwind CSS
- **Storage:** SQLite with WAL
- **Map:** AMap Web API

## Development

```sh
pnpm typecheck
pnpm lint
pnpm test
cargo test --workspace
pnpm build:mcp   # optional MCP stdio executable for development
cargo clippy --workspace --all-targets -- -D warnings
```

The repository contains a Cargo workspace (`src-tauri`, `crates/cirrus-core`, and `crates/cirrus-mcp`) and a pnpm workspace (`apps/desktop`).

## Limitations

- Release builds are not notarized; macOS may show a Gatekeeper warning.
- Linux session restore requires a Secret Service implementation.
- iCloud uses private web APIs that may change without notice.
- The device map requires an AMap key.
- Cirrus currently covers Photos, Find My, Notes, Contacts, and Calendar, but not iCloud Drive or Mail.
- MCP exposes the main Cirrus account, device, photo, download, note, contact, and calendar operations over local stdio; HTTP transport is not implemented yet.

## Trademarks and legal risk

Cirrus is an independent project. It is not affiliated with, endorsed by, sponsored by, or approved by Apple Inc.

The maintainer does not sell Cirrus or charge for its use. Non-commercial status may reduce some enforcement exposure, but it does not eliminate trademark, unfair-competition, platform-policy, or Apple service-terms risk. The Apache-2.0 license still permits downstream commercial use.

Apple, iCloud, Find My, iPhone, iPad, Mac, and AirPods are trademarks of Apple Inc. These names appear only in descriptive text to identify compatibility and supported features. Cirrus does not use Apple logos and does not present itself as an official Apple product.

Using a third-party trademark descriptively can still be challenged. In particular:

- Avoid using `iCloud` in the Cirrus product name, repository slug, domain, social handle, installer name, or logo.
- Do not use Apple logos, product artwork, or Apple-style branding in a way that suggests endorsement.
- No trademark clearance has been performed for `Cirrus`. Perform a separate trademark search before commercial distribution.
- The project uses undocumented iCloud web endpoints. This may stop working at any time and may conflict with Apple's terms of service.
- The Apache-2.0 license grants rights to the source code only; it does not grant any trademark rights.

This section is a practical notice, not legal advice. For public or commercial distribution, review Apple's trademark guidelines and consult qualified counsel.

## Disclaimer

Cirrus is an independent project and is not affiliated with, endorsed by, or sponsored by Apple Inc. It uses undocumented iCloud web endpoints and is intended for personal use and learning. Use it at your own risk and comply with Apple's terms of service.

## License

[Apache-2.0](LICENSE)
