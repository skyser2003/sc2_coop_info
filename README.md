# SC2 Coop Info

Rust/Tauri desktop overlay and replay-analysis tool for **StarCraft II Co-op**.

This repository is a modernized continuation of the original **SC2 Coop Overlay** project by **FluffyMaguro**. The goal here is to preserve the original overlay's usefulness and feature set while moving the implementation to a Rust-first stack with a Tauri desktop shell.

Original project:
- https://github.com/FluffyMaguro/SC2_Coop_Overlay

Release page for this repository:
- https://github.com/skyser2003/sc2_coop_info/releases

## Respect To The Original Project

This project exists because the original SC2 Coop Overlay was genuinely useful to the co-op community. The UI concepts, workflow, and overall product direction come from that original work. This repository focuses on maintaining and improving that experience while replacing older implementation pieces with Rust-based equivalents.

## What This Project Currently Provides

- Transparent in-game overlay window
- Config window with live settings editing
- Replay history view
- Player history view with persistent notes
- Weekly mutation tracking
- Statistics views for maps, commanders, allies, regions, difficulties, and units
- Detailed-analysis cache generation for deeper statistics
- Commander randomizer
- Performance overlay with process monitoring
- Global hotkeys for overlay controls
- System tray integration
- Native folder picker and Windows startup integration
- Rust-based replay parsing and analysis
- English and Korean support

## Current Architecture

The current app is centered around the `tauri-overlay` desktop application and Rust analysis crates:

- `tauri-overlay`
  - Tauri desktop shell
  - React + Vite frontend
  - Rust backend commands and window management
- `s2coop-analyzer`
  - replay/statistics analysis logic
  - cache generation
- `s2protocol-port`
  - SC2 replay protocol parsing support

## Main Features

### Overlay

- Shows replay summary information after games
- Supports hotkeys for show/hide and replay navigation
- Supports player-info display at game start
- Supports chart visibility and color customization

### Config App

The config window currently includes these tabs:

- `Settings`
- `Games`
- `Players`
- `Weeklies`
- `Statistics`
- `Randomizer`
- `Performance`
- `Links`

### Replay Analysis

- Reads replay data from your StarCraft II account folder
- Builds replay lists and summary tables
- Tracks players, commanders, maps, difficulties, and regions
- Supports simple analysis and detailed analysis
- Stores generated detailed-analysis cache output for richer statistics
- Includes replay chat viewing and file reveal actions

### Performance Overlay

- Separate transparent performance window
- Tracks selected processes
- Supports its own hotkey and saved geometry

## Screenshots

**Config window**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image1.en.png)

**Replay list**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image2.en.png)

**Player list**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image3.en.png)

**Weeklies list**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image4.en.png)

**Various statistics**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image5.en.png)

**Commander randomizer**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image6.en.png)

**Performance overlay**

![Screenshot](https://raw.githubusercontent.com/skyser2003/sc2_coop_info/main/readme_images/image7.en.png)

## Running The App For Development

### Prerequisites

- Rust toolchain
- Node.js and npm
- Windows is the primary target environment

### Frontend + Tauri dev run

```powershell
cd tauri-overlay
npm install
npm run tauri dev
```

## Building

```powershell
cd tauri-overlay
npm install
cargo tauri build
```

## Notes About Settings And Usage

- The app expects access to your StarCraft II account folder to analyze replays.
- The config window applies many settings live to the running overlay backend.
- `settings.json` is updated when you explicitly save settings.
- For the in-game overlay experience, StarCraft II should be run in windowed or borderless fullscreen mode.

## Windows Notes

- Windows is the main supported desktop target.
- The app includes tray behavior, global shortcuts, startup registration, and overlay window placement logic tailored for Windows use.

## Development Notes

- Frontend: React, Vite, Material UI, Tauri API
- Backend: Rust, Tauri

## Repository Status

This repository is an in-progress Rust/Tauri implementation of the original SC2 Coop Overlay functionality. Some behavior is intentionally being aligned with the original project while some older implementation details are being removed or rewritten.

## Feedback

For bugs, feedback, and suggestions, please open an issue or send an email to below address.

- mailto:sc2coopinfo@gmail.com
