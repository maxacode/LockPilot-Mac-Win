# LockPilot - Made by Maks (Windows)
Change in Github.com Manual 

![LockPilot App Screenshot](./assets/app-screenshot.png)

A Windows desktop app to schedule system actions for specific times, including recurring schedules, update channels, and rollback support.

## Install on Windows

Quick Start:
- **Stable (main):** [Download latest release](https://github.com/maxacode/LockPilot-Mac-Win/releases/latest)
- **Dev (dev):** [View all prereleases](https://github.com/maxacode/LockPilot-Mac-Win/releases)

## Features

- Multiple concurrent timers
- One-time timer execution
- Actions:
  - Popup message
  - Lock screen
  - Shut down
  - Restart/reboot
- Recurring schedules:
  - Daily
  - Weekdays
  - Every N hours (1–24)
- Cancel any active timer
- Live timer list with next run time and countdown
- Timer persistence to local app data and automatic restore on launch
- In-app updater:
  - Check now
  - Auto-check on launch toggle
  - Update channels: main (stable) and dev (prerelease)
  - Install latest build from selected channel
- Rollback:
  - Select a specific release tag/version and install it
- Glass-inspired theme with modern UI

## Windows Behavior Notes

- **Lock** uses `LockWorkStation()` Win32 API
- **Shutdown** uses `shutdown /s /t 0`
- **Reboot** uses `shutdown /r /t 0`
- **Popup** uses `MessageBoxW()` Win32 API

## Timer Persistence

Timers are saved to app data (`timers.json`) when created/updated/canceled and restored automatically on app launch.

## Project Layout

- `src-tauri/`: Rust backend + Tauri app config
- `ui/`: static frontend (HTML/CSS/JS)

## Dev Run (no JS framework required)

1. Install Rust and cargo.
2. Install Tauri CLI:
   ```
   cargo install tauri-cli
   ```
3. In one terminal, serve the `ui/` folder:
   ```
   python -m http.server 1420 --directory ui
   ```
4. In another terminal, run:
   ```
   cd src-tauri
   cargo tauri dev
   ```

## Build

From `src-tauri/`:

```
cargo tauri build --bundles nsis
```

Output bundles are under `src-tauri/target/release/bundle/`.
