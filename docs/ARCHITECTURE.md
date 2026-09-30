# KeyClick architecture

```
┌──────────── OS ────────────┐      ┌────────────── KeyClick process ───────────────┐
│ key press                  │      │                                                │
│   │                        │      │  keyboard thread            audio thread       │
│   ├──► every application   │      │  (hook / event tap)                            │
│   │    (unchanged)         │      │  classify → KeyCategory ──► pick variation     │
│   └──► listen-only hook ───┼─────►│  try_send (never blocks)    mix preloaded PCM  │
│                            │      │  vk code dropped here       → speakers         │
└────────────────────────────┘      │                              category dropped  │
                                    │  main thread: Tauri UI, tray, settings.json    │
                                    └────────────────────────────────────────────────┘
```

## Privacy by construction

| Rule | Where it is enforced |
| --- | --- |
| Events are observed, never modified/blocked/injected | Windows: `WH_KEYBOARD_LL` hook always returns `CallNextHookEx`. macOS: `kCGEventTapOptionListenOnly`. |
| Key codes never leave the OS callback | `keyboard/windows.rs`, `keyboard/macos.rs` convert to `KeyCategory` immediately. |
| No text, order or history is stored | The only per-key state is a "currently held" bit (auto-repeat filter), cleared on key-up. |
| No logging / file / network in keyboard code | Unit test `keyboard_sources_have_no_output_paths`. The app has no network code at all. |
| Settings contain preferences only | `settings/mod.rs`, test `serialized_settings_contain_no_key_data`. |
| Paused = not listening | Turning KeyClick OFF uninstalls the hook / event tap. |

On macOS, Secure Event Input (password fields, some terminals' secure keyboard entry) hides key
events from all taps, so KeyClick is silent there by OS design.

## Keyboard (src-tauri/src/keyboard)

- `key_classifier.rs` — platform key codes → `KeyCategory { Normal, Space, Enter, Backspace, Tab,
  Escape, Modifier, Arrow, Function, Other }`, plus the auto-repeat filter.
- `windows.rs` — dedicated thread installs `SetWindowsHookExW(WH_KEYBOARD_LL)` and sleeps in
  `GetMessageW` (zero CPU when idle). The callback does a table lookup and a non-blocking channel
  send, far below Windows' `LowLevelHooksTimeout`. Panics are caught so they never cross FFI.
- `macos.rs` — `CGEventTapCreate` (session tap, listen-only) on its own `CFRunLoop` thread;
  re-enables itself if macOS disables the tap by timeout. Permission via
  `CGPreflightListenEventAccess` / `CGRequestListenEventAccess` (Input Monitoring).
- The channel is bounded (64). If audio stalls, events are dropped — typing is never delayed.

## Audio (src-tauri/src/audio)

- `soundpack.rs` — discovers `pack.json` manifests, validates them (safe file names only, no
  path traversal, size/duration limits) and decodes `.ogg` (lewton) / `.wav` (hound) to mono f32
  **when a pack is selected**, never while typing.
- `engine.rs`
  - Resamples every clip to the device rate once (`PreparedPack`).
  - One `cpal` output stream, reused. A key press pushes an `Arc` to preloaded PCM into a
    lock-free `ArrayQueue`; the real-time callback mixes up to 24 overlapping voices without
    allocating, locking or touching disk.
  - `SoundSelector` picks a random variation and never repeats the previous one per category.
  - The stream pauses after 2 min of silence → no CPU use, device can sleep.
  - Device loss / default-device change / sleep-wake → stream is rebuilt automatically.
  - Every failure is contained in the audio thread (`catch_unwind`); keyboard input is unaffected.

Perceived latency ≈ OS hook delivery (<1 ms) + device buffer (typically 10–20 ms on WASAPI shared
mode / CoreAudio), within the <30 ms target. After 2 min idle, the first key also pays the stream
restart (a few ms on typical hardware).

Like a real switch, a key makes two sounds: the press (bottom-out) and a quieter release
(top-out). The hook forwards key-up as `KeyEvent { category, release: true }` — still only a
category — and only for keys it saw go down.

## Settings (src-tauri/src/settings)

`settings.json` in the platform config directory:

- Windows: `%APPDATA%\com.keyclick.app\settings.json`
- macOS: `~/Library/Application Support/com.keyclick.app/settings.json`

Written atomically (temp file + rename). Corrupt files are moved to `settings.json.bak` and
defaults are used. Values are sanitized on load (volume clamped, pack id validated).

## App shell (src-tauri/src/lib.rs, tray.rs, commands.rs)

- Single instance: launching again focuses the existing window.
- Closing the window hides it; only **Quit** in the tray/menu bar exits.
- Tray (Windows) / menu bar (macOS, template icon, no Dock icon via `LSUIElement`).
- Start at login via `tauri-plugin-autostart`: Windows `HKCU\…\Run` entry, macOS LaunchAgent.
  Off by default; only changed when the user flips the switch. Autostarts pass `--autostart`
  so KeyClick starts hidden.
- UI ↔ backend: a handful of commands (`get_state`, `update_settings`, `preview_sound`,
  permission helpers). No command accepts or returns typed content.
- Strict CSP; the webview can only reach the local app bundle and IPC.

## Updates

V1 ships without an updater and never contacts a server. Planned design:

1. Add `tauri-plugin-updater`; generate an updater key pair with `tauri signer generate`
   (private key only in CI secrets, public key in `tauri.conf.json`).
2. Host a static `latest.json` next to the installers (no backend needed):
   ```json
   { "version": "1.2.0", "notes": "…", "pub_date": "…",
     "platforms": { "windows-x86_64": { "url": "…/KeyClick-Setup.exe", "signature": "…" },
                    "darwin-universal": { "url": "…/KeyClick.app.tar.gz", "signature": "…" } } }
   ```
3. Check at most once a day, only if the user enabled "Check for updates".
4. Show a non-blocking prompt: *New version available — KeyClick 1.2.0 — [Update] [Later]*.
   Never force an update.

## Extension points

| Future feature | Hook in the code |
| --- | --- |
| Custom sound packs | Already supported: drop a folder with `pack.json` into `<app data>/soundpacks/` (see [SOUND_PACKS.md](SOUND_PACKS.md)). `SoundPackInfo.builtin` distinguishes them. |
| Downloadable / community / premium packs | Download into the same `soundpacks/` folder; manifest validation already rejects unsafe files. Add a `source`/`license` field to the manifest. |
| Cloud sync | `Settings` is a plain serde struct; sync only this file (never key data). |
| Opt-in analytics | Would live outside `keyboard/`; the privacy test forbids output paths there. |
| Android / iOS | `KeyCategory`, `soundpack` and `engine` are platform-neutral; only a new input source is needed (e.g. an Android IME / iOS keyboard extension playing its own clicks). |
