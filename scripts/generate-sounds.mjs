// Synthesizes the six built-in sound packs into src-tauri/sounds.
//
// Physical model of a key press (all original synthesis, no recordings):
//   impact exciter (short noise burst)
//     -> keycap/housing modes (many inharmonic resonators, 0.4-8 kHz, fast decay)
//     -> plate/case/desk modes (few low resonators, 120-700 Hz, slower decay)
//     + direct band-passed "clack" transient
//   Mechanical switches: click-jacket snap -> bottom-out -> tiny bounce.
//   Key release: separate, quieter, brighter "top-out" sound.
//   A short sparse (velvet-noise) room reverb puts the keyboard on a desk.
// Fixed seeds make the output reproducible. To ship recordings instead, drop
// licensed .ogg/.wav files into a pack folder and list them in pack.json.
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tauriDir } from './lib.mjs';

const RATE = 48000; // most output devices run at 48 kHz -> no resampling at runtime
const OUT = join(tauriDir, 'sounds');
const ms = (v) => v / 1000;
const smp = (msec) => Math.round(ms(msec) * RATE);

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const between = (rand, lo, hi) => lo + (hi - lo) * rand();
const jit = (rand, amount) => 1 + (rand() * 2 - 1) * amount;

function bandpass(f, q) {
  const w0 = (2 * Math.PI * Math.min(f, RATE * 0.45)) / RATE;
  const alpha = Math.sin(w0) / (2 * q);
  const a0 = 1 + alpha;
  const b0 = alpha / a0, b2 = -alpha / a0, a1 = (-2 * Math.cos(w0)) / a0, a2 = (1 - alpha) / a0;
  let x1 = 0, x2 = 0, y1 = 0, y2 = 0;
  return (x) => {
    const y = b0 * x + b2 * x2 - a1 * y1 - a2 * y2;
    x2 = x1; x1 = x; y2 = y1; y1 = y;
    return y;
  };
}

/** Two-pole resonator: rings at f with time constant tau when excited. */
function resonator(f, tauMs, gain) {
  const w = (2 * Math.PI * Math.min(f, RATE * 0.45)) / RATE;
  const r = Math.exp(-1 / (ms(tauMs) * RATE));
  const c1 = 2 * r * Math.cos(w), c2 = -r * r, norm = Math.sin(w) * gain;
  let y1 = 0, y2 = 0;
  return (x) => {
    const y = c1 * y1 + c2 * y2 + x * norm;
    y2 = y1; y1 = y;
    return y;
  };
}

/** A pack's fixed set of resonant modes (the "keyboard"), shared by all its keys. */
function makeModes(rand, { n, lo, hi, tau, g = 1 }) {
  return Array.from({ length: n }, (_, i) => {
    const f = lo * Math.pow(hi / lo, (i + rand()) / n);
    return [f, tau * Math.pow(1000 / f, 0.4) * between(rand, 0.7, 1.3), g * between(rand, 0.35, 1) / Math.sqrt(1 + i * 0.3)];
  });
}

/** Per-key variation: every key on a real board resonates slightly differently. */
const vary = (modes, rand, pitch, tauScale, amount = 0.05) =>
  modes.map(([f, tau, g]) => [f * pitch * jit(rand, amount), tau * tauScale * jit(rand, 0.15), g * jit(rand, 0.3)]);

/**
 * Adds one physical impact to `buf` at sample `at`.
 * exc: { tau (ms), lp, hp } noise burst; click: direct band-passed transient.
 */
function impact(buf, rand, at, { amp = 1, exc, click, modes = [], body = [], direct = 0.15 }) {
  const excN = Math.min(buf.length - at, smp(exc.tau * 8));
  const lpA = exc.lp ? 1 - Math.exp((-2 * Math.PI * exc.lp) / RATE) : 1;
  const hpA = exc.hp ? Math.exp((-2 * Math.PI * exc.hp) / RATE) : 0;
  const res = [...modes, ...body].map(([f, tau, g]) => resonator(f, tau, g));
  const bp = click && bandpass(click.f, click.q);
  let lp = 0, hpPrevIn = 0, hpOut = 0;
  for (let i = 0; at + i < buf.length; i++) {
    const t = i / RATE;
    let x = 0;
    if (i < excN) {
      const n = (rand() * 2 - 1) * Math.exp(-t / ms(exc.tau));
      lp += lpA * (n - lp);
      hpOut = hpA ? hpA * (hpOut + lp - hpPrevIn) : lp;
      hpPrevIn = lp;
      x = hpOut;
    }
    let s = x * direct;
    for (const r of res) s += r(x);
    if (bp) s += bp(rand() * 2 - 1) * click.g * Math.exp(-t / ms(click.tau));
    buf[at + i] += s * amp;
  }
}

/** Sparse random-tap reverb: a small room around the desk. */
function room(buf, rand, { mix = 0.1, tau = 14, len = 60 }) {
  const taps = [];
  for (let p = smp(0.8); p < smp(len); p += Math.round(between(rand, 0.4, 1.6) * RATE / 1800)) {
    taps.push([p, (rand() < 0.5 ? -1 : 1) * Math.exp(-p / (ms(tau) * RATE))]);
  }
  const norm = mix / Math.sqrt(taps.reduce((s, [, a]) => s + a * a, 0));
  const dry = Float32Array.from(buf);
  for (let n = 0; n < buf.length; n++) {
    let acc = 0;
    for (const [p, a] of taps) {
      if (p > n) break;
      acc += dry[n - p] * a;
    }
    buf[n] += acc * norm;
  }
}

/** Soft saturation, DC removal, trim of the silent tail, fades, peak normalize. */
function finish(buf, level, drive = 1.4) {
  let px = 0, py = 0;
  const r = 1 - (2 * Math.PI * 25) / RATE;
  for (let i = 0; i < buf.length; i++) {
    const y = buf[i] - px + r * py;
    px = buf[i]; py = y; buf[i] = y;
  }
  let peak = buf.reduce((m, v) => Math.max(m, Math.abs(v)), 1e-9);
  for (let i = 0; i < buf.length; i++) buf[i] = Math.tanh((drive * buf[i]) / peak) / Math.tanh(drive);
  let end = buf.length;
  while (end > smp(10) && Math.abs(buf[end - 1]) < 0.0015) end--;
  const out = buf.slice(0, Math.min(buf.length, end + smp(2)));
  const fadeIn = smp(0.08), fadeOut = Math.min(smp(8), out.length >> 2);
  for (let i = 0; i < fadeIn; i++) out[i] *= i / fadeIn;
  for (let i = 0; i < fadeOut; i++) out[out.length - 1 - i] *= i / fadeOut;
  peak = out.reduce((m, v) => Math.max(m, Math.abs(v)), 1e-9);
  for (let i = 0; i < out.length; i++) out[i] = (out[i] / peak) * level;
  return out;
}

function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((v, i) => data.writeInt16LE(Math.round(Math.max(-1, Math.min(1, v)) * 32767), i * 2));
  const h = Buffer.alloc(44);
  h.write('RIFF', 0); h.writeUInt32LE(36 + data.length, 4); h.write('WAVE', 8);
  h.write('fmt ', 12); h.writeUInt32LE(16, 16); h.writeUInt16LE(1, 20); h.writeUInt16LE(1, 22);
  h.writeUInt32LE(RATE, 24); h.writeUInt32LE(RATE * 2, 28); h.writeUInt16LE(2, 32); h.writeUInt16LE(16, 34);
  h.write('data', 36); h.writeUInt32LE(data.length, 40);
  return Buffer.concat([h, data]);
}

// Bigger keys are lower, longer and rattle (stabilizer wire).
const KINDS = {
  normal: { pitch: 1, tau: 1, amp: 1, rattle: 0 },
  space: { pitch: 0.62, tau: 1.6, amp: 1, rattle: 0.35 },
  enter: { pitch: 0.8, tau: 1.3, amp: 1, rattle: 0.18 },
  backspace: { pitch: 0.86, tau: 1.2, amp: 1, rattle: 0.12 },
  tab: { pitch: 0.9, tau: 1.15, amp: 1, rattle: 0 },
  modifier: { pitch: 0.94, tau: 1.1, amp: 0.85, rattle: 0 },
};

/**
 * Builds a physically modelled pack from a description of its impacts.
 * spec.press / spec.release: [{ at, jitter, amp, exc, click, modes: key, body }]
 */
function physicalPack(seed, spec) {
  const rand = mulberry32(seed);
  const modeSets = Object.fromEntries(Object.entries(spec.modes).map(([k, v]) => [k, makeModes(rand, v)]));
  const rattleModes = makeModes(rand, { n: 5, lo: 1900, hi: 4600, tau: 14, g: 0.7 });

  const render = (rand, hits, kind, len, level) => {
    const k = KINDS[kind];
    const buf = new Float32Array(smp(len * k.tau));
    let t = 0;
    for (const h of hits) {
      t = h.at === undefined ? t : h.at * k.tau + (h.jitter ? between(rand, -h.jitter, h.jitter) : 0);
      const scale = (f) => f * k.pitch * jit(rand, 0.04);
      impact(buf, rand, smp(Math.max(0, t)), {
        amp: h.amp * k.amp,
        exc: h.exc,
        direct: h.direct,
        click: h.click && { ...h.click, f: scale(h.click.f), tau: h.click.tau * k.tau },
        modes: h.modes ? vary(modeSets[h.modes], rand, k.pitch, k.tau) : [],
        body: (h.body || []).map(([f, tau, g]) => [scale(f), tau * k.tau * jit(rand, 0.1), g * (kind === 'space' ? 1.4 : 1)]),
      });
    }
    if (k.rattle) {
      for (const dt of [2 + rand() * 2, 5 + rand() * 3]) {
        impact(buf, rand, smp(dt), { amp: k.rattle, exc: { tau: 0.15, hp: 1500 }, modes: vary(rattleModes, rand, 1, 1, 0.08) });
      }
    }
    room(buf, rand, spec.room);
    return finish(buf, level * (spec.levels?.[kind] ?? 1), spec.drive);
  };

  return {
    press: (rand, kind) => render(rand, spec.press, kind, spec.len, spec.level),
    release: (rand) => render(rand, spec.release, 'normal', spec.releaseLen ?? spec.len * 0.7, spec.level * spec.releaseLevel),
  };
}

/** Square-wave "chip" blip with an exponential pitch sweep, bit-crushed. */
function addChip(out, { f0, f1, dur, at = 0, duty = 0.5, gain = 1 }) {
  const start = smp(at);
  const n = Math.min(out.length - start, smp(dur));
  let phase = 0, held = 0;
  for (let i = 0; i < n; i++) {
    const k = i / n;
    phase += (f0 * Math.pow(f1 / f0, k)) / RATE;
    if (i % 4 === 0) held = (phase % 1 < duty ? 1 : -1) * (1 - k);
    out[start + i] += (Math.round(held * 8) / 8) * gain;
  }
}

function retroPack() {
  return {
    press: (rand, kind) => {
      const j = 1 + (rand() - 0.5) * 0.18;
      const spec = {
        normal: [{ f0: 1320 * j, f1: 700 * j, dur: 28 }],
        space: [{ f0: 520 * j, f1: 260 * j, dur: 55, duty: 0.25 }],
        enter: [{ f0: 660, f1: 660, dur: 45 }, { f0: 990, f1: 990, dur: 60, at: 45 }],
        backspace: [{ f0: 900 * j, f1: 300 * j, dur: 50, duty: 0.25 }],
        tab: [{ f0: 780 * j, f1: 1170 * j, dur: 40 }],
        modifier: [{ f0: 1600 * j, f1: 1500 * j, dur: 14, gain: 0.7 }],
      }[kind];
      const len = Math.max(...spec.map((s) => (s.at || 0) + s.dur)) + 5;
      const buf = new Float32Array(smp(len));
      spec.forEach((s) => addChip(buf, s));
      let peak = buf.reduce((m, v) => Math.max(m, Math.abs(v)), 1e-9);
      return buf.map((v) => (v / peak) * (kind === 'modifier' ? 0.3 : 0.42));
    },
    release: null,
  };
}

const mechanical = physicalPack(11, {
  // Clicky tactile switch (click jacket), ABS caps on a steel plate.
  modes: {
    jacket: { n: 6, lo: 2600, hi: 7800, tau: 2.2, g: 0.6 },
    cap: { n: 12, lo: 850, hi: 6200, tau: 6.5, g: 1 },
    top: { n: 9, lo: 1300, hi: 6800, tau: 4.5, g: 0.9 },
  },
  press: [
    { at: 0, amp: 0.5, exc: { tau: 0.1, hp: 2500 }, click: { f: 5400, q: 1.1, g: 0.7, tau: 0.5 }, modes: 'jacket' },
    { at: 6.5, jitter: 1.5, amp: 1, exc: { tau: 0.3, lp: 9000 }, click: { f: 3300, q: 0.8, g: 0.35, tau: 0.8 }, modes: 'cap',
      body: [[210, 20, 0.45], [345, 13, 0.4], [560, 9, 0.32], [1150, 5, 0.25]] },
    { at: 8, jitter: 0.4, amp: 0.22, exc: { tau: 0.2, lp: 8000 }, modes: 'cap' },
  ],
  release: [
    { at: 0, amp: 1, exc: { tau: 0.22, lp: 10000 }, click: { f: 3900, q: 0.9, g: 0.35, tau: 0.6 }, modes: 'top', body: [[270, 11, 0.3], [480, 7, 0.25]] },
    { at: 3, jitter: 0.8, amp: 0.45, exc: { tau: 0.1, hp: 3000 }, modes: 'jacket' },
  ],
  len: 90, level: 0.85, releaseLevel: 0.55, room: { mix: 0.12, tau: 12, len: 55 }, drive: 1.5,
});

const thock = physicalPack(23, {
  // Lubed switches, thick PBT caps, gasket mount: deep, round, muted.
  modes: {
    cap: { n: 10, lo: 420, hi: 2700, tau: 11, g: 1 },
    top: { n: 8, lo: 650, hi: 3300, tau: 7, g: 0.8 },
  },
  press: [
    { at: 0, amp: 1, exc: { tau: 0.55, lp: 3200 }, click: { f: 1700, q: 0.7, g: 0.2, tau: 1 }, direct: 0.08, modes: 'cap',
      body: [[135, 34, 0.85], [225, 24, 0.65], [380, 15, 0.45], [610, 10, 0.3]] },
    { at: 1.6, jitter: 0.3, amp: 0.18, exc: { tau: 0.4, lp: 3000 }, modes: 'cap' },
  ],
  release: [
    { at: 0, amp: 1, exc: { tau: 0.4, lp: 4500 }, direct: 0.08, modes: 'top', body: [[180, 18, 0.4], [320, 12, 0.3]] },
  ],
  len: 110, level: 0.9, releaseLevel: 0.45, room: { mix: 0.14, tau: 16, len: 70 }, drive: 1.3,
});

const linear = physicalPack(37, {
  // Smooth linear switches, ABS caps, plate mount: crisp clack, no click.
  modes: {
    cap: { n: 11, lo: 700, hi: 4600, tau: 7, g: 1 },
    top: { n: 9, lo: 1100, hi: 5600, tau: 5, g: 0.9 },
  },
  press: [
    { at: 0, amp: 1, exc: { tau: 0.28, lp: 7500 }, click: { f: 2800, q: 0.8, g: 0.3, tau: 0.7 }, modes: 'cap',
      body: [[190, 19, 0.5], [310, 13, 0.42], [520, 9, 0.35], [900, 6, 0.25]] },
    { at: 1.2, jitter: 0.3, amp: 0.22, exc: { tau: 0.2, lp: 7000 }, modes: 'cap' },
  ],
  release: [
    { at: 0, amp: 1, exc: { tau: 0.2, lp: 9000 }, click: { f: 3600, q: 0.9, g: 0.3, tau: 0.6 }, modes: 'top', body: [[240, 11, 0.3], [430, 7, 0.22]] },
  ],
  len: 85, level: 0.8, releaseLevel: 0.55, room: { mix: 0.12, tau: 12, len: 55 }, drive: 1.4,
});

const typewriterBase = physicalPack(41, {
  // Key lever, then the steel typebar striking paper on the rubber platen.
  modes: {
    lever: { n: 6, lo: 1300, hi: 4200, tau: 3, g: 0.6 },
    bar: { n: 14, lo: 1100, hi: 7200, tau: 22, g: 0.8 },
    escape: { n: 5, lo: 2400, hi: 6500, tau: 3, g: 0.5 },
  },
  press: [
    { at: 0, amp: 0.3, exc: { tau: 0.2, lp: 7000 }, modes: 'lever' },
    { at: 11, jitter: 2, amp: 1, exc: { tau: 0.22 }, click: { f: 3000, q: 0.9, g: 0.9, tau: 1.2 }, modes: 'bar',
      body: [[118, 26, 0.6], [255, 18, 0.5], [470, 12, 0.4]] },
    { at: 34, jitter: 3, amp: 0.2, exc: { tau: 0.12, hp: 2000 }, modes: 'escape' },
  ],
  release: [
    { at: 0, amp: 1, exc: { tau: 0.35, lp: 5000 }, modes: 'lever', body: [[200, 14, 0.35], [390, 9, 0.25]] },
  ],
  len: 130, level: 0.85, releaseLevel: 0.35, room: { mix: 0.16, tau: 20, len: 80 }, drive: 1.5,
});

const typewriter = {
  press: (rand, kind) => {
    if (kind !== 'enter') return typewriterBase.press(rand, kind);
    // Carriage return: the strike, then the margin bell.
    const strike = typewriterBase.press(rand, 'normal');
    const buf = new Float32Array(smp(700));
    buf.set(strike.subarray(0, buf.length));
    const f = 1865;
    impact(buf, rand, smp(45), { amp: 0.5, exc: { tau: 0.15 }, direct: 0, modes: [[f, 320, 1], [f * 2.76, 140, 0.45], [f * 5.4, 70, 0.2]] });
    return finish(buf, 0.8, 1.2);
  },
  release: typewriterBase.release,
};

const soft = physicalPack(53, {
  // Silent (dampened) switches: a muffled, gentle tap.
  modes: {
    cap: { n: 7, lo: 300, hi: 1500, tau: 6, g: 0.8 },
    top: { n: 5, lo: 450, hi: 1800, tau: 4, g: 0.6 },
  },
  press: [
    { at: 0, amp: 1, exc: { tau: 0.9, lp: 1700 }, direct: 0.2, modes: 'cap', body: [[160, 14, 0.6], [280, 10, 0.5]] },
  ],
  release: [
    { at: 0, amp: 1, exc: { tau: 0.7, lp: 2000 }, direct: 0.2, modes: 'top', body: [[210, 9, 0.35]] },
  ],
  len: 60, level: 0.5, releaseLevel: 0.4, room: { mix: 0.08, tau: 10, len: 40 }, drive: 1.1,
});

const PACKS = [
  { id: 'mechanical', name: 'Mechanical', description: 'Crisp, clicky mechanical switches', synth: mechanical },
  { id: 'thock', name: 'Thock', description: 'Deep, muted, lubed-switch thock', synth: thock },
  { id: 'linear', name: 'Linear', description: 'Smooth, clacky linear switches', synth: linear },
  { id: 'retro', name: 'Retro', description: '8-bit home-computer blips', synth: retroPack() },
  { id: 'typewriter', name: 'Typewriter', description: 'Classic manual typewriter, with bell', synth: typewriter },
  { id: 'soft', name: 'Soft', description: 'Gentle, dampened silent switches', synth: soft },
];

const NORMAL_VARIATIONS = 8;
const RELEASE_VARIATIONS = 4;
const SPECIALS = ['space', 'enter', 'backspace', 'tab', 'modifier'];

rmSync(OUT, { recursive: true, force: true });
let total = 0;
PACKS.forEach((pack, packIndex) => {
  const dir = join(OUT, pack.id);
  mkdirSync(dir, { recursive: true });
  const manifest = { id: pack.id, name: pack.name, description: pack.description, normal_keys: [], release_keys: [] };
  const write = (file, samples) => {
    const bytes = wav(samples);
    writeFileSync(join(dir, file), bytes);
    total += bytes.length;
  };
  const seed = (n) => mulberry32(1000 * (packIndex + 1) + n);
  for (let v = 0; v < NORMAL_VARIATIONS; v++) {
    const file = `key${String(v + 1).padStart(2, '0')}.wav`;
    write(file, pack.synth.press(seed(v), 'normal'));
    manifest.normal_keys.push(file);
  }
  if (pack.synth.release) {
    for (let v = 0; v < RELEASE_VARIATIONS; v++) {
      const file = `release${String(v + 1).padStart(2, '0')}.wav`;
      write(file, pack.synth.release(seed(200 + v)));
      manifest.release_keys.push(file);
    }
  }
  SPECIALS.forEach((kind, i) => {
    write(`${kind}.wav`, pack.synth.press(seed(100 + i), kind));
    manifest[kind] = `${kind}.wav`;
  });
  writeFileSync(join(dir, 'pack.json'), JSON.stringify(manifest, null, 2) + '\n');
  console.log(`  ${pack.id.padEnd(11)} ${NORMAL_VARIATIONS} press + ${manifest.release_keys.length} release variations + ${SPECIALS.length} special keys`);
});
console.log(`Sound packs written to ${OUT} (${(total / 1024).toFixed(0)} KiB total)`);
