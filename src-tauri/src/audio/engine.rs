//! Low-latency audio engine.
//!
//! Design:
//! * All sounds are decoded and resampled to the device rate ahead of time.
//! * One output stream is created and reused; each key press only pushes a tiny
//!   `Voice` (an `Arc` to preloaded PCM + gain) into a lock-free queue.
//! * The real-time callback mixes up to [`MAX_VOICES`] overlapping voices with
//!   no allocation, locking or I/O.
//! * The stream is paused after a period of silence so the idle app uses no
//!   CPU and does not keep the audio device (or the system) awake.
//! * Any audio failure is contained here: the keyboard side only ever does a
//!   non-blocking channel send, so typing is never affected.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use crossbeam_channel::{select, Receiver, Sender};
use crossbeam_queue::ArrayQueue;
use serde::Serialize;

use super::soundpack::{LoadedPack, PreparedPack};
use crate::keyboard::{KeyCategory, KeyEvent};
use crate::settings::Settings;

pub const MAX_VOICES: usize = 24;
// Long enough that pauses while thinking don't make the next key pay a stream restart.
const IDLE_PAUSE: Duration = Duration::from_secs(120);
const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(3);
const REBUILD_RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// A request to play one preloaded clip.
pub struct Voice {
    pub samples: Arc<[f32]>,
    pub gain: f32,
}

struct ActiveVoice {
    samples: Arc<[f32]>,
    gain: f32,
    pos: usize,
}

/// Real-time mixer run inside the audio callback.
pub struct Mixer {
    queue: Arc<ArrayQueue<Voice>>,
    voices: Vec<ActiveVoice>,
}

impl Mixer {
    pub fn new(queue: Arc<ArrayQueue<Voice>>) -> Self {
        Self { queue, voices: Vec::with_capacity(MAX_VOICES) }
    }

    pub fn render<T: SizedSample + FromSample<f32>>(&mut self, out: &mut [T], channels: usize) {
        while let Some(v) = self.queue.pop() {
            if self.voices.len() == MAX_VOICES {
                // Steal the oldest voice rather than growing.
                self.voices.remove(0);
            }
            self.voices.push(ActiveVoice { samples: v.samples, gain: v.gain, pos: 0 });
        }
        let channels = channels.max(1);
        for frame in out.chunks_mut(channels) {
            let mut acc = 0.0f32;
            for v in self.voices.iter_mut() {
                if let Some(s) = v.samples.get(v.pos) {
                    acc += *s * v.gain;
                    v.pos += 1;
                }
            }
            let sample = T::from_sample(acc.clamp(-1.0, 1.0));
            frame.fill(sample);
        }
        self.voices.retain(|v| v.pos < v.samples.len());
    }

    #[cfg(test)]
    fn active(&self) -> usize {
        self.voices.len()
    }
}

/// Picks a variation index, avoiding immediate repetition per slot
/// (one slot per key category, plus one for key releases).
#[derive(Default)]
pub struct SoundSelector {
    last: [Option<usize>; 11],
}

fn slot(event: KeyEvent) -> usize {
    if event.release {
        10
    } else {
        KeyCategory::ALL.iter().position(|c| *c == event.category).unwrap_or(0)
    }
}

impl SoundSelector {
    pub fn pick(&mut self, slot: usize, pool_len: usize, randomize: bool, rng: &mut fastrand::Rng) -> Option<usize> {
        if pool_len == 0 {
            return None;
        }
        let slot = slot.min(self.last.len() - 1);
        let idx = if !randomize || pool_len == 1 {
            0
        } else {
            match self.last[slot] {
                // Draw from the other (n - 1) entries: uniform and never a repeat.
                Some(prev) if prev < pool_len => {
                    let r = rng.usize(..pool_len - 1);
                    if r >= prev { r + 1 } else { r }
                }
                _ => rng.usize(..pool_len),
            }
        };
        self.last[slot] = Some(idx);
        Some(idx)
    }
}

/// Turns categories into voices according to the current settings. Pure logic,
/// independent of any audio device.
pub struct Voicer {
    settings: Settings,
    pack: Option<Arc<LoadedPack>>,
    prepared: Option<(u32, PreparedPack)>,
    selector: SoundSelector,
    rng: fastrand::Rng,
}

impl Voicer {
    pub fn new(settings: Settings) -> Self {
        Self { settings, pack: None, prepared: None, selector: SoundSelector::default(), rng: fastrand::Rng::new() }
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn set_pack(&mut self, pack: Arc<LoadedPack>) {
        self.pack = Some(pack);
        self.prepared = None;
        self.selector = SoundSelector::default();
    }

    pub fn should_play(&self, event: KeyEvent) -> bool {
        self.settings.enabled
            && self.settings.category_enabled(event.category)
            && (!event.release || self.settings.release_sound)
    }

    /// Resamples the active pack for `sample_rate` if not already done.
    pub fn prepare(&mut self, sample_rate: u32) {
        let stale = !matches!(&self.prepared, Some((rate, _)) if *rate == sample_rate);
        if stale {
            self.prepared = self.pack.as_ref().map(|p| (sample_rate, p.prepare(sample_rate)));
        }
    }

    /// `force` bypasses the enabled/category switches (used for previews).
    pub fn voice(&mut self, event: KeyEvent, force: bool) -> Option<Voice> {
        if !force && !self.should_play(event) {
            return None;
        }
        let volume = crate::settings::clamp_volume(self.settings.volume);
        if volume <= 0.0 {
            return None;
        }
        let (_, prepared) = self.prepared.as_ref()?;
        let pool = if event.release { prepared.release_pool() } else { prepared.pool(event.category) };
        let idx = self.selector.pick(slot(event), pool.len(), self.settings.randomize, &mut self.rng)?;
        Some(Voice { samples: pool[idx].clone(), gain: volume * prepared.gain })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AudioStatus {
    pub ready: bool,
    pub device: Option<String>,
    pub error: Option<String>,
    pub pack: Option<String>,
}

enum Control {
    Settings(Settings),
    Pack(Arc<LoadedPack>),
    Preview(KeyCategory),
}

/// Handle to the audio thread. Cheap to clone-send messages; never blocks.
pub struct AudioEngine {
    tx: Sender<Control>,
    status: Arc<Mutex<AudioStatus>>,
    _thread: Option<JoinHandle<()>>,
}

impl AudioEngine {
    pub fn start(settings: Settings, keys: Receiver<KeyEvent>) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let status = Arc::new(Mutex::new(AudioStatus::default()));
        let thread_status = status.clone();
        let thread = std::thread::Builder::new()
            .name("keyclick-audio".into())
            .spawn(move || EngineThread::new(settings, thread_status).run(keys, rx))
            .ok();
        if thread.is_none() {
            set_status(&status, |s| s.error = Some("Could not start audio thread.".into()));
        }
        Self { tx, status, _thread: thread }
    }

    pub fn update_settings(&self, settings: Settings) {
        let _ = self.tx.send(Control::Settings(settings));
    }

    pub fn set_pack(&self, pack: Arc<LoadedPack>) {
        let _ = self.tx.send(Control::Pack(pack));
    }

    pub fn preview(&self, category: KeyCategory) {
        let _ = self.tx.send(Control::Preview(category));
    }

    pub fn status(&self) -> AudioStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn set_status(status: &Mutex<AudioStatus>, f: impl FnOnce(&mut AudioStatus)) {
    if let Ok(mut s) = status.lock() {
        f(&mut s);
    }
}

struct Output {
    stream: cpal::Stream,
    queue: Arc<ArrayQueue<Voice>>,
    failed: Arc<AtomicBool>,
    sample_rate: u32,
    device_id: Option<String>,
    playing: bool,
}

impl Output {
    fn pause(&mut self) {
        if self.playing {
            let _ = self.stream.pause();
            self.playing = false;
        }
    }
}

struct EngineThread {
    voicer: Voicer,
    output: Option<Output>,
    status: Arc<Mutex<AudioStatus>>,
    last_sound: Instant,
    last_device_check: Instant,
    last_build_attempt: Option<Instant>,
}

impl EngineThread {
    fn new(settings: Settings, status: Arc<Mutex<AudioStatus>>) -> Self {
        let now = Instant::now();
        Self {
            voicer: Voicer::new(settings),
            output: None,
            status,
            last_sound: now,
            last_device_check: now,
            last_build_attempt: None,
        }
    }

    fn run(mut self, keys: Receiver<KeyEvent>, control: Receiver<Control>) {
        // Open the device up front so the first key press is not delayed.
        self.ensure_output();
        loop {
            let playing = self.output.as_ref().is_some_and(|o| o.playing);
            let event = if playing {
                let wait = IDLE_PAUSE.saturating_sub(self.last_sound.elapsed());
                select! {
                    recv(keys) -> k => k.ok().map(Event::Key),
                    recv(control) -> c => Some(c.map_or(Event::Shutdown, Event::Control)),
                    default(wait) => Some(Event::Idle),
                }
            } else {
                select! {
                    recv(keys) -> k => k.ok().map(Event::Key),
                    recv(control) -> c => Some(c.map_or(Event::Shutdown, Event::Control)),
                }
            };
            let Some(event) = event else {
                // Keyboard side is gone for good; nothing more to do.
                break;
            };
            if matches!(event, Event::Shutdown) {
                break;
            }
            // A bug in sound handling must never take the engine down.
            let _ = catch_unwind(AssertUnwindSafe(|| self.handle(event)));
        }
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Key(key) => {
                if self.voicer.should_play(key) {
                    self.play(key, false);
                }
            }
            Event::Control(Control::Settings(s)) => {
                let enabled = s.enabled;
                self.voicer.set_settings(s);
                if !enabled {
                    if let Some(o) = self.output.as_mut() {
                        o.pause();
                    }
                }
            }
            Event::Control(Control::Pack(pack)) => {
                let id = pack.id.clone();
                self.voicer.set_pack(pack);
                if let Some(rate) = self.output.as_ref().map(|o| o.sample_rate) {
                    self.voicer.prepare(rate);
                }
                set_status(&self.status, |s| s.pack = Some(id));
            }
            Event::Control(Control::Preview(category)) => self.play(KeyEvent::press(category), true),
            Event::Idle => {
                if let Some(o) = self.output.as_mut() {
                    o.pause();
                }
            }
            Event::Shutdown => {}
        }
    }

    fn play(&mut self, key: KeyEvent, force: bool) {
        self.ensure_output();
        let Some(rate) = self.output.as_ref().map(|o| o.sample_rate) else { return };
        self.voicer.prepare(rate);
        let Some(voice) = self.voicer.voice(key, force) else { return };
        let Some(out) = self.output.as_mut() else { return };
        let _ = out.queue.force_push(voice);
        if !out.playing {
            match out.stream.play() {
                Ok(()) => out.playing = true,
                Err(_) => out.failed.store(true, Ordering::Relaxed),
            }
        }
        self.last_sound = Instant::now();
    }

    /// (Re)opens the default output device when missing, failed, or changed.
    fn ensure_output(&mut self) {
        let now = Instant::now();
        let mut rebuild = match &self.output {
            None => true,
            Some(o) => o.failed.load(Ordering::Relaxed),
        };
        if !rebuild && now.duration_since(self.last_device_check) >= DEVICE_CHECK_INTERVAL {
            self.last_device_check = now;
            let current = default_device_id();
            rebuild = self.output.as_ref().is_some_and(|o| o.device_id != current);
        }
        if !rebuild {
            return;
        }
        if self.last_build_attempt.is_some_and(|t| now.duration_since(t) < REBUILD_RETRY_INTERVAL) {
            return;
        }
        self.last_build_attempt = Some(now);
        self.last_device_check = now;
        self.output = None; // release the old device first

        match open_output() {
            Ok((output, name)) => {
                self.voicer.prepare(output.sample_rate);
                self.output = Some(output);
                set_status(&self.status, |s| {
                    s.ready = true;
                    s.device = Some(name);
                    s.error = None;
                });
            }
            Err(e) => set_status(&self.status, |s| {
                s.ready = false;
                s.device = None;
                s.error = Some(e);
            }),
        }
    }
}

enum Event {
    Key(KeyEvent),
    Control(Control),
    Idle,
    Shutdown,
}

fn default_device_id() -> Option<String> {
    cpal::default_host().default_output_device().and_then(|d| d.id().ok()).map(|id| id.to_string())
}

fn open_output() -> Result<(Output, String), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("No audio output device found.")?;
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Default output".into());
    let device_id = device.id().ok().map(|id| id.to_string());
    let supported = device.default_output_config().map_err(|e| format!("Audio device unavailable: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();

    let queue = Arc::new(ArrayQueue::new(MAX_VOICES * 2));
    let failed = Arc::new(AtomicBool::new(false));

    let stream = match format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, &queue, &failed),
        SampleFormat::I16 => build_stream::<i16>(&device, &config, &queue, &failed),
        SampleFormat::U16 => build_stream::<u16>(&device, &config, &queue, &failed),
        SampleFormat::I32 => build_stream::<i32>(&device, &config, &queue, &failed),
        SampleFormat::F64 => build_stream::<f64>(&device, &config, &queue, &failed),
        SampleFormat::I8 => build_stream::<i8>(&device, &config, &queue, &failed),
        SampleFormat::U8 => build_stream::<u8>(&device, &config, &queue, &failed),
        other => return Err(format!("Unsupported audio sample format: {other}")),
    }?;
    // Streams may auto-start on some backends; stay silent until a key is pressed.
    let _ = stream.pause();

    Ok((
        Output { stream, queue, failed, sample_rate: config.sample_rate, device_id, playing: false },
        name,
    ))
}

fn build_stream<T: SizedSample + FromSample<f32> + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: &Arc<ArrayQueue<Voice>>,
    failed: &Arc<AtomicBool>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let mut mixer = Mixer::new(queue.clone());
    let failed = failed.clone();
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| mixer.render(data, channels),
            move |err| {
                // Buffer underruns are transient glitches; anything else needs a rebuild.
                if !matches!(err, cpal::StreamError::BufferUnderrun) {
                    failed.store(true, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(|e| format!("Could not open audio stream: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::soundpack::{discover, tests::make_pack};

    fn loaded_pack(normal: usize, with_space: bool) -> Arc<LoadedPack> {
        let tmp = tempfile::tempdir().unwrap();
        make_pack(tmp.path(), "t", normal, with_space);
        let entry = discover(&[(tmp.path().to_path_buf(), true)]).remove(0);
        Arc::new(LoadedPack::load(&entry).unwrap())
    }

    fn voicer(settings: Settings, normal: usize) -> Voicer {
        let mut v = Voicer::new(settings);
        v.set_pack(loaded_pack(normal, true));
        v.prepare(48_000);
        v
    }

    const fn down(c: KeyCategory) -> KeyEvent {
        KeyEvent::press(c)
    }

    const NORMAL: usize = 0;

    #[test]
    fn selector_never_repeats_immediately_when_randomized() {
        let mut sel = SoundSelector::default();
        let mut rng = fastrand::Rng::with_seed(7);
        let mut prev = None;
        let mut seen = [0usize; 5];
        for _ in 0..5_000 {
            let i = sel.pick(NORMAL, 5, true, &mut rng).unwrap();
            assert_ne!(Some(i), prev);
            seen[i] += 1;
            prev = Some(i);
        }
        assert!(seen.iter().all(|n| *n > 700), "all variations used: {seen:?}");
    }

    #[test]
    fn selector_is_deterministic_without_randomize() {
        let mut sel = SoundSelector::default();
        let mut rng = fastrand::Rng::with_seed(1);
        for _ in 0..10 {
            assert_eq!(sel.pick(NORMAL, 5, false, &mut rng), Some(0));
        }
        let space = slot(down(KeyCategory::Space));
        assert_eq!(sel.pick(space, 1, true, &mut rng), Some(0));
        assert_eq!(sel.pick(space, 0, true, &mut rng), None);
    }

    #[test]
    fn selector_tracks_slots_independently() {
        let mut sel = SoundSelector::default();
        let mut rng = fastrand::Rng::with_seed(3);
        let a = sel.pick(NORMAL, 2, true, &mut rng).unwrap();
        let _ = sel.pick(slot(down(KeyCategory::Enter)), 2, true, &mut rng);
        let _ = sel.pick(slot(KeyEvent::release(KeyCategory::Normal)), 2, true, &mut rng);
        let b = sel.pick(NORMAL, 2, true, &mut rng).unwrap();
        assert_ne!(a, b);
        assert_ne!(slot(KeyEvent::release(KeyCategory::Normal)), NORMAL);
    }

    #[test]
    fn voicer_respects_enable_switch() {
        let mut v = voicer(Settings { enabled: false, ..Default::default() }, 3);
        assert!(v.voice(down(KeyCategory::Normal), false).is_none());
        assert!(v.voice(down(KeyCategory::Normal), true).is_some(), "preview still works");
        v.set_settings(Settings::default());
        assert!(v.voice(down(KeyCategory::Normal), false).is_some());
    }

    #[test]
    fn voicer_applies_volume() {
        let mut v = voicer(Settings { volume: 0.25, ..Default::default() }, 3);
        let voice = v.voice(down(KeyCategory::Normal), false).unwrap();
        assert!((voice.gain - 0.25).abs() < 1e-6);
        v.set_settings(Settings { volume: 0.0, ..Default::default() });
        assert!(v.voice(down(KeyCategory::Normal), false).is_none(), "muted plays nothing");
        v.set_settings(Settings { volume: 9.0, ..Default::default() });
        assert!((v.voice(down(KeyCategory::Normal), false).unwrap().gain - 1.0).abs() < 1e-6);
    }

    #[test]
    fn voicer_respects_category_toggles_and_dedicated_sounds() {
        let mut v = voicer(Settings { space_sound: false, ..Default::default() }, 3);
        assert!(v.voice(down(KeyCategory::Space), false).is_none());
        assert!(v.voice(down(KeyCategory::Modifier), false).is_none(), "modifier off by default");
        v.set_settings(Settings::default());
        let space = v.voice(down(KeyCategory::Space), false).unwrap();
        assert_eq!(space.samples.len(), 960, "dedicated space sound");
        let enter = v.voice(down(KeyCategory::Enter), false).unwrap();
        assert_eq!(enter.samples.len(), 480, "falls back to normal pool");
    }

    #[test]
    fn voicer_plays_release_sounds() {
        let mut v = voicer(Settings::default(), 3);
        let up = v.voice(KeyEvent::release(KeyCategory::Normal), false).unwrap();
        assert_eq!(up.samples.len(), 240, "uses the release pool, not the press pool");
        assert!(v.voice(KeyEvent::release(KeyCategory::Modifier), false).is_none(), "category toggle applies");
        v.set_settings(Settings { release_sound: false, ..Default::default() });
        assert!(v.voice(KeyEvent::release(KeyCategory::Normal), false).is_none());
        assert!(v.voice(down(KeyCategory::Normal), false).is_some());
    }

    #[test]
    fn voicer_without_pack_is_silent() {
        let mut v = Voicer::new(Settings::default());
        v.prepare(48_000);
        assert!(v.voice(down(KeyCategory::Normal), false).is_none());
    }

    #[test]
    fn mixer_plays_and_overlaps_voices() {
        let queue = Arc::new(ArrayQueue::new(8));
        let mut mixer = Mixer::new(queue.clone());
        let clip: Arc<[f32]> = vec![0.5f32; 4].into();
        queue.push(Voice { samples: clip.clone(), gain: 1.0 }).ok().unwrap();
        queue.push(Voice { samples: clip.clone(), gain: 0.5 }).ok().unwrap();
        let mut out = vec![0.0f32; 12]; // 6 stereo frames
        mixer.render(&mut out, 2);
        assert_eq!(&out[..8], &[0.75; 8]);
        assert_eq!(&out[8..], &[0.0; 4]);
        assert_eq!(mixer.active(), 0, "finished voices are released");
    }

    #[test]
    fn mixer_clamps_and_converts() {
        let queue = Arc::new(ArrayQueue::new(8));
        let mut mixer = Mixer::new(queue.clone());
        let clip: Arc<[f32]> = vec![0.9f32; 2].into();
        for _ in 0..3 {
            queue.push(Voice { samples: clip.clone(), gain: 1.0 }).ok().unwrap();
        }
        let mut out = vec![0i16; 2];
        mixer.render(&mut out, 1);
        assert_eq!(out, [i16::MAX, i16::MAX]);
    }

    #[test]
    #[ignore = "needs a real audio output device"]
    fn engine_opens_device_and_plays_quickly() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let engine = AudioEngine::start(Settings::default(), rx);
        engine.set_pack(loaded_pack(3, true));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !(engine.status().ready && engine.status().pack.is_some()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = engine.status();
        assert!(status.ready, "audio error: {:?}", status.error);
        for i in 0..200 {
            let e = if i % 2 == 0 { down(KeyCategory::Normal) } else { KeyEvent::release(KeyCategory::Normal) };
            tx.try_send(e).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(engine.status().ready, "survives very fast typing");
    }

    #[test]
    fn mixer_caps_polyphony() {
        let queue = Arc::new(ArrayQueue::new(MAX_VOICES * 4));
        let mut mixer = Mixer::new(queue.clone());
        let clip: Arc<[f32]> = vec![0.01f32; 100].into();
        for _ in 0..MAX_VOICES * 3 {
            let _ = queue.push(Voice { samples: clip.clone(), gain: 1.0 });
        }
        let mut out = vec![0.0f32; 1];
        mixer.render(&mut out, 1);
        assert_eq!(mixer.active(), MAX_VOICES);
    }
}
