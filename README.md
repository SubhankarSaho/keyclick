# KeyClick

> Make typing sound satisfying.

KeyClick is a tiny desktop app for **Windows** and **macOS** that plays short keyboard sounds
while you type — in any app. It runs quietly in the tray / menu bar and works alongside your
existing keyboard.

- **Never replaces your keyboard.** Key events are observed in listen-only mode and passed on unchanged.
- **Not a keylogger.** Each key press becomes a generic category (NORMAL, SPACE, ENTER…), triggers a sound and is discarded. Nothing is stored, logged or uploaded.
- **Offline.** No account, no backend, no telemetry, no network code.
- **Lightweight.** Rust + Tauri (no Electron). ~0 % CPU when idle; sounds are preloaded in memory.

## Download (for normal users)

| Platform | File to download |
| --- | --- |
| Windows 10 / 11 | **`KeyClick-Setup.exe`** (recommended) · `KeyClick.msi` for IT deployment |
| macOS (Apple Silicon & Intel) | **`KeyClick.dmg`** |

Step-by-step instructions: [docs/INSTALLATION.md](docs/INSTALLATION.md).

## Features

- Six built-in sound packs: Mechanical, Thock, Linear, Retro, Typewriter, Soft
- Random variations with no immediate repeats, plus key-release ("top-out") sounds like a real switch
- Separate sounds for Space, Enter, Backspace, Tab and modifiers (each can be turned off)
- Master ON/OFF switch — when OFF, the keyboard listener is removed completely
- Volume 0–100 % (default 40 %)
- Tray (Windows) / menu bar (macOS) menu: Enable, Sound Pack, Volume, Settings, Pause, Quit
- Optional start at login (off by default)
- First-run onboarding, macOS permission guide, local test area
- Custom sound packs: [docs/SOUND_PACKS.md](docs/SOUND_PACKS.md)

## Development

Requirements: Rust (stable, 1.80+), Node.js 20+.
Windows: MSVC build tools *or* the GNU toolchain (MinGW-w64), WebView2 (preinstalled on Win 10/11).
macOS: Xcode command line tools.

```bash
npm install
npm run app:dev        # run the app with hot reload
npm test               # Rust tests + TypeScript type-check
npm run test:hardware  # + real keyboard-hook and audio-device tests
npm run build-windows  # -> release/KeyClick-Setup.exe, release/KeyClick.msi   (on Windows)
npm run build-macos    # -> release/KeyClick.dmg, release/KeyClick.app         (on macOS)
npm run build-all      # builds whatever the current OS supports
npm run clean          # remove dist/, release/ and Cargo build output
npm run sounds         # regenerate the built-in sound packs
npm run icons          # regenerate all icons from src-tauri/icons/source/*.svg
```

Release, signing and notarization: [docs/RELEASE.md](docs/RELEASE.md).

## Project layout

```
keyclick/
├── src-tauri/                 Rust backend (Tauri 2)
│   ├── src/
│   │   ├── main.rs            entry point
│   │   ├── lib.rs             app state, startup, window/tray lifecycle
│   │   ├── commands.rs        UI commands (settings/status only)
│   │   ├── tray.rs            system tray / menu bar
│   │   ├── audio/             engine.rs (mixer, voices), soundpack.rs (load/decode)
│   │   ├── keyboard/          key_classifier.rs, windows.rs (WH_KEYBOARD_LL), macos.rs (CGEventTap)
│   │   └── settings/          mod.rs (model), persistence.rs (atomic JSON)
│   ├── sounds/<pack>/         pack.json + audio files
│   ├── icons/                 generated app/tray icons (+ source SVGs)
│   ├── capabilities/          Tauri permissions for the UI
│   └── tauri.conf.json        bundling: NSIS, MSI, app, DMG
├── src/                       React + TypeScript UI (pages, components, styles)
├── scripts/                   build, test, clean, sound/icon generators
├── docs/                      installation, architecture, release, testing, sound packs
└── website/index.html         download landing page
```

Architecture and privacy design: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Known limitations

- **Verified on Windows only.** This build environment is Windows (GNU toolchain). Tested:
  unit tests, the real hook/audio hardware tests, release build, NSIS + MSI bundling, silent
  per-user install → launch → uninstall, onboarding, settings persistence, start-with-Windows
  (HKCU Run key added/removed), close-to-tray, single instance, ~0 ms CPU over 10 s idle.
  Not yet tested by hand: listening to every pack in other apps, sleep/wake, Bluetooth keyboards
  (see [docs/TESTING.md](docs/TESTING.md)).
- **macOS has not been built or tested.** The macOS code (`keyboard/macos.rs`, menu bar,
  LaunchAgent, DMG config) is written but not compiled here.
  macOS build requires macOS/Xcode for final native validation and signing.
- Windows builds are unsigned until a code-signing certificate is configured → SmartScreen warning.
- Windows: no sounds in windows of elevated (administrator) apps, on the secure desktop (UAC,
  login screen) — Windows does not deliver those events to a normal-user hook. Input is unaffected.
- macOS: no sounds in password fields / Secure Keyboard Entry (OS-enforced). A newly granted
  Input Monitoring permission may need one app restart; unsigned dev builds must be re-approved
  after each rebuild.
- Built-in sounds are synthesized (original, license-free). Swap in licensed recordings for a
  more realistic feel — see [docs/SOUND_PACKS.md](docs/SOUND_PACKS.md).
- Tray volume offers presets (0–100 % in 20 % steps); the window has a continuous slider.
- No auto-update in V1 (design documented in ARCHITECTURE.md).

## License

MIT. Built-in sounds and icons are original works generated by the scripts in `scripts/`.
