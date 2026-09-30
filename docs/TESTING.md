# Testing

## Automated

```
npm test
```

Runs `cargo test` (Rust unit tests) and the TypeScript type-check. Coverage:

| Area | Tests |
| --- | --- |
| Key classification | `keyboard::key_classifier::tests` — Windows VK codes, macOS key codes, generic labels, auto-repeat filter |
| Privacy | `keyboard::tests::keyboard_sources_have_no_output_paths`, `settings::tests::serialized_settings_contain_no_key_data` |
| Non-blocking input path | `keyboard::tests::sink_never_blocks_when_full_or_disconnected` |
| Sound selection / variation | `audio::engine::tests::selector_*` (never repeats, uses all variations, per-category) |
| Volume / enable / toggles | `audio::engine::tests::voicer_*`, `settings::tests::volume_is_clamped`, `category_toggles`, `tray::tests` |
| Mixer | `mixer_plays_and_overlaps_voices`, `mixer_clamps_and_converts`, `mixer_caps_polyphony` |
| Sound-pack loading | `audio::soundpack::tests::*` incl. path-traversal rejection, fallbacks, and `bundled_packs_are_complete_and_short` (loads the six shipped packs) |
| Settings persistence | `settings::persistence::tests::*` (roundtrip, corrupt file, sanitizing) |

### Hardware tests (opt-in)

```
npm run test:hardware
```

Also runs the `#[ignore]`d tests that touch the real machine:

- `keyboard::windows::tests::hook_delivers_categories_and_filters_repeat` — installs the real
  `WH_KEYBOARD_LL` hook and injects **F24** (a key no keyboard or app uses): checks categories
  arrive, auto-repeat is filtered and nothing arrives after unhooking.
- `audio::engine::tests::engine_opens_device_and_plays_quickly` — opens the default output device
  and plays 200 clicks at 5 ms intervals (audible for ~1 s).

## Manual checklist

Record results per release. For every app: type normally, confirm characters, shortcuts
(Ctrl/Cmd+C/V/Z, Alt+Tab / Cmd+Tab), IME input and auto-repeat behave exactly as without KeyClick.

### Windows 10 / 11

| Scenario | Expected |
| --- | --- |
| Notepad, Chrome/Edge, VS Code, Word, Slack, Windows Terminal | Normal input, one sound per key press |
| Windows login / UAC prompt / password fields | Input unaffected. (Secure desktop: no sounds — hooks don't run there.) |
| Games / elevated apps (run as admin) | Input unaffected. Low-level hooks don't see elevated windows from a non-elevated KeyClick → silent there. |
| Hold a key | One sound, no machine-gun repeat |
| Very fast typing / key mashing | No lag, no stuck keys, overlapping sounds |
| OFF switch / tray Pause | Silence immediately; input unaffected |
| Close window | App stays in tray |
| Start with Windows ON → sign out/in | KeyClick starts hidden in tray |
| Sleep / wake, unplug headphones, switch output device | Sound resumes within a few seconds |
| USB and Bluetooth keyboard, disconnect/reconnect | Works on both, keeps working after reconnect |
| Volume 0 / system muted | Silent; input unaffected |
| Uninstall | Removed from Start menu, Apps list, Run key |
| Privacy | `%APPDATA%\com.keyclick.app` contains only `settings.json` (preferences). `%LOCALAPPDATA%\com.keyclick.app\EBWebView` is the WebView2 cache for the settings UI. |

### macOS (Apple Silicon + Intel)

| Scenario | Expected |
| --- | --- |
| First launch | Welcome → permission screen → Open System Settings opens Input Monitoring |
| Grant permission | Screen advances (or offers Restart KeyClick) → sounds play |
| Deny / revoke permission | Banner "Keyboard access required"; keyboard unaffected |
| TextEdit, Safari, Chrome, VS Code, Terminal | Normal input, sounds |
| Password fields / Terminal "Secure Keyboard Entry" | Input unaffected, no sounds (OS blocks taps) |
| Menu-bar icon | Menu works; no Dock icon; closing window keeps app running |
| Launch at Login ON → log out/in | Starts hidden in menu bar |
| Sleep / wake, AirPods connect/disconnect | Sound resumes |

## Performance checks

- Idle (window closed, not typing): Task Manager / Activity Monitor shows ~0 % CPU.
- Typing at >15 keys/s for a minute: CPU stays low, no audio dropouts, no input delay.
