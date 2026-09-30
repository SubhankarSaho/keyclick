# Sound packs

## Built-in packs

`src-tauri/sounds/<pack>/` — Mechanical, Thock, Linear, Retro, Typewriter, Soft.

All built-in sounds are **synthesized from scratch** by `scripts/generate-sounds.mjs` using a
physical model (noise-burst impact exciting keycap/housing resonances and low plate/case modes,
click-jacket → bottom-out → bounce timing, stabilizer rattle on big keys, a short desk-room
reverb). They contain no recordings, so there are no third-party rights involved. Regenerate with:

```
npm run sounds
```

Each pack has 8 key-press variations, 4 key-release ("top-out") variations, and dedicated
space, enter, backspace, tab and modifier sounds. Files are 16-bit mono 48 kHz WAV (the usual
device rate, so no runtime resampling), 60–140 ms including the natural decay. WAV is used for
the synthesized packs because the files are tiny and need no decoding; `.ogg` is fully
supported for recorded packs.

## Replacing with recorded / licensed sounds

**Only use recordings you made yourself or have a license to redistribute.**
Do not copy sounds from other apps or websites.

1. Record or license short clips: normal keys 10–60 ms, special keys up to ~300 ms.
   Trim silence at the start — any leading silence is added latency.
2. Export as `.ogg` (Vorbis, mono, 44.1 or 48 kHz) or `.wav`. Max 4 MB and 2 s per file.
3. Put them in the pack folder, e.g. `src-tauri/sounds/mechanical/`.
4. Update `pack.json`:

```json
{
  "id": "mechanical",
  "name": "Mechanical",
  "description": "Crisp mechanical keyboard",
  "normal_keys": ["key01.ogg", "key02.ogg", "key03.ogg", "key04.ogg", "key05.ogg"],
  "release_keys": ["up01.ogg", "up02.ogg", "up03.ogg"],
  "space": "space.ogg",
  "enter": "enter.ogg",
  "backspace": "backspace.ogg",
  "tab": "tab.ogg",
  "modifier": "modifier.ogg",
  "gain": 1.0
}
```

5. Record the source and license of the files in your release notes.
6. `npm test` — the `bundled_packs_are_complete_and_short` test checks every pack decodes.

Optional keys: `release_keys` (played when a key comes back up; without it releases are silent),
`escape`, `arrow`, `function`. Any missing special sound falls back to the normal-key
variations. `gain` (0.0–2.0) balances loudness between packs.

**Tip for the most realistic sound:** record your own keyboard with a phone about 30 cm away in
a quiet room, cut the key-down and key-up clicks into separate files (start each file right at
the transient), and use 5–10 variations of each.

## Custom packs (user-installed)

Users can add packs without rebuilding the app. Create a folder with a `pack.json` and sound
files in:

- Windows: `%APPDATA%\com.keyclick.app\soundpacks\<my-pack>\`
- macOS: `~/Library/Application Support/com.keyclick.app/soundpacks/<my-pack>/`

Restart KeyClick and the pack appears in the list. Rules: `id` uses `a-z 0-9 - _`, file names
must be plain names inside the folder (no paths), up to 32 normal-key variations. Invalid packs
are skipped. A custom pack cannot override a built-in pack id.
