# Teltonika Orchestrator

[![Release](https://img.shields.io/github/v/release/ulxl/teltonika-orchestrator?color=blue&label=version)](https://github.com/ulxl/teltonika-orchestrator/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows-blue.svg)](https://github.com/ulxl/teltonika-orchestrator)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

High-Performance Native Desktop Automation & Fleet Management Engine for Teltonika Networks devices (RUT, TRB, and TAP series), built with **Rust**, **Tokio**, **Axum**, and **Wry (WebView2)**.

> **Personal Project Notice:** This application is developed as an independent personal automation utility. It is not an official Teltonika product and is provided as-is under the MIT License.

---

## Architecture Overview

Teltonika Orchestrator combines the raw performance, concurrency, and memory safety of native Rust with a modern, responsive web-based UI embedded via WebView2:

- **Asynchronous Concurrency Engine**: Powered by `tokio` multi-threading to orchestrate batch actions across dozens of devices simultaneously with customizable worker concurrency (1–50 threads).
- **Embedded Web Server**: Local `axum` micro-server hosting the single-page application interface, REST API endpoints, and real-time Server-Sent Events (SSE) telemetry.
- **Dual Communication Protocol**: Seamlessly supports both RutOS UBUS JSON-RPC commands and the modern RutOS REST API, automatically detecting endpoint capabilities.
- **Native File Reports**: Generates professional Excel spreadsheets directly with `rust_xlsxwriter`.
- **Zero-Dependency Portable Binary**: Compiles to a single standalone `teltonika-orchestrator.exe` executable with embedded assets and in-place GitHub Releases auto-updating.

---

## Key Features

### 1. General Fleet Automation
- **Data Extraction**: Pulls hardware identifiers, firmware versions, serial numbers, MAC addresses, and cellular parameters (IMEI, ICCID, IMSI, Operator, RSSI, RSRP, RSRQ, SINR).
- **Password Standardization**: Validates and updates device administrative passwords across fleets to enforce security compliance.
- **Backup & Restore**: Automatically downloads configuration archives (`.tar.gz`) or batch-deploys backup configuration packages.
- **Firmware Upgrades**: Transfers firmware files over SCP/SSH and executes upgrade sequences with automated reboot tracking.
- **Single-Click Reboot**: Instantly dispatches reboot signals to individual devices or batch selections.

### 2. Access Point (AP) Operations
- **Wi-Fi Credential Audit**: Pulls SSIDs, radio configurations, and pre-shared keys across dual-band and multi-interface setups.
- **MAC Whitelist Extraction**: Displays active whitelist filtering rules per radio with deduplication.
- **Safe Non-Destructive MAC Insertion**: Safely appends new MAC addresses to access point whitelists without overwriting existing permitted clients.

### 3. Multi-Port Bundle Diagnostics
- **Batch Port Scanning**: Validates device responsiveness across multiple port mappings.
- **RMS Reconnection Trigger**: Forces device re-registration with the Remote Management System (RMS).

### 4. User Experience & Desktop Features
- **In-App Auto-Updates**: One-click update check and in-place executable replacement directly from GitHub Releases without manual re-downloading.
- **Dark & Light Mode**: Automatic system preference detection with manual toggle.
- **Real-Time Terminal Console**: Live streaming event logs with tag filtering (All, Info, Success, Warning, Error) and full-text search.
- **Interactive Data Table**: Drag-to-resize columns, status pill badges, and one-click clipboard copying.
- **Customizable Workspace**: Configurable output directories for logs, templates, and generated reports.

---

## Quick Start

### Option A: Download Pre-Built Executable (Recommended)
1. Head to the [Latest Releases](https://github.com/ulxl/teltonika-orchestrator/releases/latest) page.
2. Download `teltonika-orchestrator.exe`.
3. Run `teltonika-orchestrator.exe` (requires Microsoft Edge WebView2 runtime, pre-installed on Windows 10/11).

### Option B: Build from Source
Ensure you have the Rust toolchain installed:

```powershell
git clone https://github.com/ulxl/teltonika-orchestrator.git
cd teltonika-orchestrator
cargo build --release
```

The optimized executable will be located at:
```
target/release/teltonika-orchestrator.exe
```

---

## In-App Auto-Updates

The application includes an in-place updater powered by GitHub Releases:
1. Open **Settings** (gear icon in the top header).
2. Click **Check for Updates**.
3. If a newer release is published on GitHub, release notes and an **Update Now** button will appear.
4. Clicking update downloads the new binary, swaps executables atomically, and relaunches the updated application automatically.

---

## Configuration & Credentials

All configuration and custom templates are stored locally in the workspace directory under `config/templates.json`:
- **Default Authentication Candidates**: `qwerty123`, `qwerty123!`.
- **Default Ports**: `443` (Router & AP HTTPS), `22` (SSH / SCP). Fully customizable in the Settings modal.
- Passwords and device IP inputs are stored locally on your machine and never transmitted to external services.

---

## Project Structure

```
├── .github/workflows/
│   └── release.yml        # Automated Windows build & GitHub Release pipeline
├── config/
│   └── templates.json     # Default password templates and app preferences
├── src/
│   ├── config.rs          # Workspace and configuration lifecycle management
│   ├── excel.rs           # Multi-sheet Excel report generator
│   ├── lib.rs             # Module definitions
│   ├── main.rs            # Application entrypoint & Wry WebView2 window
│   ├── server.rs          # Axum HTTP routes, SSE streaming & API handlers
│   ├── teltonika.rs       # Teltonika RutOS UBUS/REST automation engine
│   └── updater.rs         # GitHub Releases version checking & in-place update engine
├── ui/
│   ├── app.js             # Frontend reactive logic & streaming handlers
│   └── index.html         # Single-page interface & design system
├── Cargo.toml             # Rust package manifest & dependencies
└── LICENSE                # MIT License
```

---

## Author & License

- **Developer**: [Hilmy](https://github.com/ulxl)
- **License**: Released under the [MIT License](LICENSE).
