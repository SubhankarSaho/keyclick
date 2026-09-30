//! Sound-pack discovery, validation, decoding and preparation.
//!
//! All decoding happens when a pack is selected — never while typing. The
//! audio engine only ever touches in-memory PCM buffers.

use std::collections::HashSet;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::keyboard::KeyCategory;
use crate::settings::is_valid_pack_id;

pub const MANIFEST_FILE: &str = "pack.json";
/// Keep key sounds short; anything longer is truncated.
pub const MAX_CLIP_SECONDS: f32 = 2.0;
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_VARIATIONS: usize = 32;
/// Display order for the bundled packs.
pub const BUILTIN_ORDER: [&str; 6] = ["mechanical", "thock", "linear", "retro", "typewriter", "soft"];

fn default_gain() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundPackManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub normal_keys: Vec<String>,
    /// Key-up ("top-out") variations. Optional: without them releases are silent.
    #[serde(default)]
    pub release_keys: Vec<String>,
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub enter: Option<String>,
    #[serde(default)]
    pub backspace: Option<String>,
    #[serde(default)]
    pub tab: Option<String>,
    #[serde(default)]
    pub escape: Option<String>,
    #[serde(default)]
    pub modifier: Option<String>,
    #[serde(default)]
    pub arrow: Option<String>,
    #[serde(default)]
    pub function: Option<String>,
    /// Per-pack loudness trim, 0.0 ..= 2.0.
    #[serde(default = "default_gain")]
    pub gain: f32,
}

impl SoundPackManifest {
    fn special(&self, category: KeyCategory) -> Option<&str> {
        match category {
            KeyCategory::Space => self.space.as_deref(),
            KeyCategory::Enter => self.enter.as_deref(),
            KeyCategory::Backspace => self.backspace.as_deref(),
            KeyCategory::Tab => self.tab.as_deref(),
            KeyCategory::Escape => self.escape.as_deref(),
            KeyCategory::Modifier => self.modifier.as_deref(),
            KeyCategory::Arrow => self.arrow.as_deref(),
            KeyCategory::Function => self.function.as_deref(),
            KeyCategory::Normal | KeyCategory::Other => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_pack_id(&self.id) {
            return Err(format!("invalid pack id '{}'", self.id));
        }
        if self.normal_keys.is_empty() {
            return Err("normal_keys must contain at least one file".into());
        }
        if self.normal_keys.len() > MAX_VARIATIONS || self.release_keys.len() > MAX_VARIATIONS {
            return Err(format!("at most {MAX_VARIATIONS} variations per key type"));
        }
        let specials = KeyCategory::ALL.iter().filter_map(|c| self.special(*c));
        for name in self.normal_keys.iter().chain(&self.release_keys).map(String::as_str).chain(specials) {
            if !is_safe_file_name(name) {
                return Err(format!("invalid sound file name '{name}'"));
            }
        }
        if !(0.0..=2.0).contains(&self.gain) {
            return Err("gain must be between 0.0 and 2.0".into());
        }
        Ok(())
    }
}

/// Only plain file names inside the pack folder are allowed (no traversal).
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains(['/', '\\', ':'])
        && name != "."
        && name != ".."
        && !name.starts_with('.')
}

#[derive(Debug, Clone, Serialize)]
pub struct SoundPackInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
}

#[derive(Debug, Clone)]
pub struct PackEntry {
    pub manifest: SoundPackManifest,
    pub dir: PathBuf,
    pub builtin: bool,
}

impl PackEntry {
    pub fn info(&self) -> SoundPackInfo {
        SoundPackInfo {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            builtin: self.builtin,
        }
    }
}

/// Scans `roots` (in priority order) for `<root>/<pack>/pack.json`.
/// Invalid packs are skipped; the first pack with a given id wins.
pub fn discover(roots: &[(PathBuf, bool)]) -> Vec<PackEntry> {
    let mut seen = HashSet::new();
    let mut packs = Vec::new();
    for (root, builtin) in roots {
        let Ok(entries) = fs::read_dir(root) else { continue };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            let Ok(text) = fs::read_to_string(dir.join(MANIFEST_FILE)) else { continue };
            let Ok(manifest) = serde_json::from_str::<SoundPackManifest>(&text) else { continue };
            if manifest.validate().is_err() || !seen.insert(manifest.id.clone()) {
                continue;
            }
            packs.push(PackEntry { manifest, dir, builtin: *builtin });
        }
    }
    packs.sort_by_key(|p| {
        let rank = BUILTIN_ORDER.iter().position(|id| *id == p.manifest.id).unwrap_or(usize::MAX);
        (rank, p.manifest.name.to_lowercase())
    });
    packs
}

/// Mono PCM clip at its native sample rate.
#[derive(Debug, Clone)]
pub struct Clip {
    pub samples: Arc<[f32]>,
    pub sample_rate: u32,
}

impl Clip {
    #[cfg(test)]
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate.max(1) as f32
    }
}

/// A decoded pack held fully in memory.
#[derive(Debug, Clone)]
pub struct LoadedPack {
    pub id: String,
    pub gain: f32,
    pub normal: Vec<Clip>,
    pub release: Vec<Clip>,
    special: [Option<Clip>; 10],
}

fn category_index(category: KeyCategory) -> usize {
    KeyCategory::ALL.iter().position(|c| *c == category).unwrap_or(0)
}

impl LoadedPack {
    /// Decodes every file of the pack. Missing/broken special sounds fall back
    /// to the normal-key sounds; at least one normal sound must load.
    pub fn load(entry: &PackEntry) -> Result<Self, String> {
        let m = &entry.manifest;
        m.validate()?;
        let decode_all = |files: &[String]| -> Vec<Clip> {
            files.iter().filter_map(|f| decode_file(&entry.dir.join(f)).ok()).collect()
        };
        let normal = decode_all(&m.normal_keys);
        if normal.is_empty() {
            return Err(format!("sound pack '{}' has no playable normal-key sounds", m.id));
        }
        let release = decode_all(&m.release_keys);
        let mut special: [Option<Clip>; 10] = Default::default();
        for category in KeyCategory::ALL {
            if let Some(file) = m.special(category) {
                special[category_index(category)] = decode_file(&entry.dir.join(file)).ok();
            }
        }
        Ok(Self { id: m.id.clone(), gain: m.gain, normal, release, special })
    }

    #[cfg(test)]
    pub fn has_dedicated(&self, category: KeyCategory) -> bool {
        self.special[category_index(category)].is_some()
    }

    /// Resamples every clip to the output device rate, once, ahead of time.
    pub fn prepare(&self, sample_rate: u32) -> PreparedPack {
        let convert = |c: &Clip| -> Arc<[f32]> {
            resample_linear(&c.samples, c.sample_rate, sample_rate).into()
        };
        PreparedPack {
            gain: self.gain,
            normal: self.normal.iter().map(convert).collect(),
            release: self.release.iter().map(convert).collect(),
            special: std::array::from_fn(|i| self.special[i].as_ref().map(convert)),
        }
    }
}

/// A pack converted to the output sample rate, ready for zero-cost playback.
pub struct PreparedPack {
    pub gain: f32,
    normal: Vec<Arc<[f32]>>,
    release: Vec<Arc<[f32]>>,
    special: [Option<Arc<[f32]>>; 10],
}

impl PreparedPack {
    /// The variation pool for a category (dedicated sound, or the normal pool).
    pub fn pool(&self, category: KeyCategory) -> &[Arc<[f32]>] {
        match &self.special[category_index(category)] {
            Some(clip) => std::slice::from_ref(clip),
            None => &self.normal,
        }
    }

    /// Key-up variations; empty if the pack has none.
    pub fn release_pool(&self) -> &[Arc<[f32]>] {
        &self.release
    }
}

pub fn decode_file(path: &Path) -> Result<Clip, String> {
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!("{} is too large", path.display()));
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    let (interleaved, channels, rate) = match ext.as_str() {
        "wav" => decode_wav(path)?,
        "ogg" => decode_ogg(path)?,
        other => return Err(format!("unsupported audio format '{other}'")),
    };
    if channels == 0 || rate == 0 {
        return Err("invalid audio header".into());
    }
    let mut mono = downmix(&interleaved, channels);
    mono.truncate((MAX_CLIP_SECONDS * rate as f32) as usize);
    if mono.is_empty() {
        return Err(format!("{} contains no audio", path.display()));
    }
    Ok(Clip { samples: mono.into(), sample_rate: rate })
}

fn decode_wav(path: &Path) -> Result<(Vec<f32>, usize, u32), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>()
        }
    }
    .map_err(|e| e.to_string())?;
    Ok((samples, spec.channels as usize, spec.sample_rate))
}

fn decode_ogg(path: &Path) -> Result<(Vec<f32>, usize, u32), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let channels = reader.ident_hdr.audio_channels as usize;
    let rate = reader.ident_hdr.audio_sample_rate;
    let limit = (MAX_CLIP_SECONDS * rate as f32) as usize * channels.max(1);
    let mut out = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| e.to_string())? {
        out.extend(packet.into_iter().map(|s| s as f32 / 32768.0));
        if out.len() >= limit {
            break;
        }
    }
    Ok((out, channels, rate))
}

fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 || to == 0 {
        return input.to_vec();
    }
    let out_len = ((input.len() as u64 * to as u64) / from as u64).max(1) as usize;
    let step = from as f64 / to as f64;
    let last = input.len() - 1;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * step;
            let idx = (pos as usize).min(last);
            let frac = (pos - idx as f64) as f32;
            let a = input[idx];
            let b = input[(idx + 1).min(last)];
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize, amplitude: f32) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..frames * channels as usize {
            w.write_sample((amplitude * i16::MAX as f32) as i16).unwrap();
        }
        w.finalize().unwrap();
    }

    pub fn make_pack(root: &Path, id: &str, normal: usize, with_space: bool) -> PathBuf {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        let mut names = Vec::new();
        for i in 0..normal {
            let n = format!("key{:02}.wav", i + 1);
            write_wav(&dir.join(&n), 44_100, 1, 441, 0.5);
            names.push(n);
        }
        let mut manifest = serde_json::json!({
            "id": id, "name": id.to_uppercase(), "description": "test", "normal_keys": names,
            "release_keys": ["up01.wav"],
        });
        write_wav(&dir.join("up01.wav"), 48_000, 1, 240, 0.2);
        if with_space {
            write_wav(&dir.join("space.wav"), 48_000, 2, 960, 0.25);
            manifest["space"] = "space.wav".into();
        }
        fs::write(dir.join(MANIFEST_FILE), manifest.to_string()).unwrap();
        dir
    }

    #[test]
    fn manifest_parses_spec_example() {
        let json = r#"{
            "id": "mechanical", "name": "Mechanical", "description": "Crisp mechanical keyboard",
            "normal_keys": ["key01.ogg","key02.ogg","key03.ogg","key04.ogg","key05.ogg"],
            "space": "space.ogg", "enter": "enter.ogg", "backspace": "backspace.ogg",
            "tab": "tab.ogg", "modifier": "modifier.ogg"
        }"#;
        let m: SoundPackManifest = serde_json::from_str(json).unwrap();
        assert!(m.validate().is_ok());
        assert_eq!(m.normal_keys.len(), 5);
        assert_eq!(m.gain, 1.0);
        assert_eq!(m.special(KeyCategory::Space), Some("space.ogg"));
        assert_eq!(m.special(KeyCategory::Arrow), None);
    }

    #[test]
    fn manifest_rejects_path_traversal() {
        let mut m: SoundPackManifest = serde_json::from_str(
            r#"{"id":"x","name":"X","normal_keys":["../../secret.wav"]}"#,
        )
        .unwrap();
        assert!(m.validate().is_err());
        m.normal_keys = vec!["ok.wav".into()];
        m.space = Some("C:\\evil.wav".into());
        assert!(m.validate().is_err());
        m.space = None;
        m.id = "Bad Id".into();
        assert!(m.validate().is_err());
        m.id = "x".into();
        assert!(m.validate().is_ok());
    }

    #[test]
    fn discover_orders_builtins_and_skips_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        make_pack(tmp.path(), "zeta", 1, false);
        make_pack(tmp.path(), "thock", 2, false);
        make_pack(tmp.path(), "mechanical", 2, false);
        fs::create_dir_all(tmp.path().join("broken")).unwrap();
        fs::write(tmp.path().join("broken").join(MANIFEST_FILE), "nope").unwrap();

        let packs = discover(&[(tmp.path().to_path_buf(), true)]);
        let ids: Vec<_> = packs.iter().map(|p| p.manifest.id.as_str()).collect();
        assert_eq!(ids, ["mechanical", "thock", "zeta"]);
    }

    #[test]
    fn discover_first_root_wins_on_duplicate_id() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        make_pack(a.path(), "soft", 1, false);
        make_pack(b.path(), "soft", 3, false);
        let packs = discover(&[(a.path().to_path_buf(), true), (b.path().to_path_buf(), false)]);
        assert_eq!(packs.len(), 1);
        assert!(packs[0].builtin);
    }

    #[test]
    fn load_decodes_and_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        make_pack(tmp.path(), "p", 3, true);
        let entry = discover(&[(tmp.path().to_path_buf(), true)]).remove(0);
        let pack = LoadedPack::load(&entry).unwrap();
        assert_eq!(pack.normal.len(), 3);
        assert!(pack.has_dedicated(KeyCategory::Space));
        assert!(!pack.has_dedicated(KeyCategory::Enter));
        // Stereo file is downmixed to mono.
        let prepared = pack.prepare(48_000);
        assert_eq!(prepared.pool(KeyCategory::Space).len(), 1);
        assert_eq!(prepared.pool(KeyCategory::Space)[0].len(), 960);
        assert_eq!(prepared.pool(KeyCategory::Enter).len(), 3, "falls back to normal pool");
        // 441 frames @ 44.1k -> 480 frames @ 48k
        assert_eq!(prepared.pool(KeyCategory::Normal)[0].len(), 480);
    }

    #[test]
    fn load_skips_missing_files_but_requires_one_normal() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = make_pack(tmp.path(), "p", 2, false);
        fs::remove_file(dir.join("key01.wav")).unwrap();
        let entry = discover(&[(tmp.path().to_path_buf(), true)]).remove(0);
        assert_eq!(LoadedPack::load(&entry).unwrap().normal.len(), 1);
        fs::remove_file(dir.join("key02.wav")).unwrap();
        assert!(LoadedPack::load(&entry).is_err());
    }

    #[test]
    fn decode_rejects_unknown_format() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("x.mp3");
        fs::write(&p, b"abc").unwrap();
        assert!(decode_file(&p).is_err());
    }

    #[test]
    fn bundled_packs_are_complete_and_short() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sounds");
        let packs = discover(&[(root, true)]);
        let ids: Vec<_> = packs.iter().map(|p| p.manifest.id.as_str()).collect();
        assert_eq!(ids, BUILTIN_ORDER, "all six built-in packs ship, in order");
        for entry in &packs {
            let pack = LoadedPack::load(entry).unwrap();
            assert!(pack.normal.len() >= 3, "{}: needs variations", pack.id);
            assert_eq!(pack.normal.len(), entry.manifest.normal_keys.len(), "{}: every file decodes", pack.id);
            for clip in &pack.normal {
                assert!(clip.duration_secs() <= 0.15, "{}: normal key too long", pack.id);
                assert!(clip.samples.iter().any(|s| s.abs() > 0.1), "{}: clip is silent", pack.id);
            }
            for c in [KeyCategory::Space, KeyCategory::Enter, KeyCategory::Backspace] {
                assert!(pack.has_dedicated(c), "{}: missing {c:?}", pack.id);
            }
            assert_eq!(pack.release.len(), entry.manifest.release_keys.len(), "{}: releases decode", pack.id);
            if pack.id != "retro" {
                assert!(pack.release.len() >= 3, "{}: needs key-up sounds", pack.id);
            }
        }
    }

    #[test]
    fn resample_preserves_duration_and_values() {
        let input = vec![0.5f32; 441];
        let out = resample_linear(&input, 44_100, 48_000);
        assert_eq!(out.len(), 480);
        assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6));
        assert_eq!(resample_linear(&input, 48_000, 48_000).len(), 441);
        let ramp: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let down = resample_linear(&ramp, 2, 1);
        assert_eq!(down.len(), 50);
        assert_eq!(down[10], 20.0);
    }
}
