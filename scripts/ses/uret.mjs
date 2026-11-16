// Zody ses seti: beş kısa bildirim sesi, tamamen burada sentezlenir (örnek/dış kaynak yok).
// Çalıştır: node scripts/ses/uret.mjs → src-tauri/sounds/*.wav (32 kHz, mono, 16 bit PCM).
// Tını: yumuşak, cam gibi bir zil (temel + oktav + hızlı sönen uyumsuz kısmi), kısa atak, üstel sönüm.

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const RATE = 32000;
const OUT = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src-tauri', 'sounds');

/** Bir nota: başlangıç (sn), frekans (Hz), sönüm süresi (sn), güç; isteğe bağlı perde kayması ve sertlik. */
function bell(buf, { at, f, decay, amp = 1, glideFrom = 1, glideMs = 25, hard = 0 }) {
  const start = Math.round(at * RATE);
  const len = Math.round(decay * 5 * RATE);
  let phase1 = 0, phase2 = 0, phase3 = 0, phase4 = 0;
  for (let i = 0; i < len && start + i < buf.length; i++) {
    const t = i / RATE;
    // Notanın başında perde hafifçe yukarıdan iner: "tık" yerine yuvarlak bir "bop".
    const g = t * 1000 < glideMs ? glideFrom + (1 - glideFrom) * (t * 1000 / glideMs) : 1;
    const fr = f * g;
    phase1 += (2 * Math.PI * fr) / RATE;
    phase2 += (2 * Math.PI * fr * 2) / RATE;
    phase3 += (2 * Math.PI * fr * 2.76) / RATE;
    phase4 += (2 * Math.PI * fr * 3) / RATE;
    const attack = Math.min(1, t / 0.004);
    const env = attack * Math.exp(-t / decay);
    const sparkle = Math.exp(-t / (decay * 0.25));
    const s =
      Math.sin(phase1) +
      0.28 * Math.sin(phase2) * Math.exp(-t / (decay * 0.6)) +
      0.12 * Math.sin(phase3) * sparkle +
      hard * Math.sin(phase4) * Math.exp(-t / (decay * 0.5));
    buf[start + i] += amp * env * s;
  }
}

/** Yükselen kısa "bwip": selamın başı. */
function sweep(buf, { at, from, to, ms, amp = 1 }) {
  const start = Math.round(at * RATE);
  const len = Math.round((ms / 1000) * RATE * 2.2);
  let phase = 0;
  for (let i = 0; i < len && start + i < buf.length; i++) {
    const t = i / RATE;
    const k = Math.min(1, (t * 1000) / ms);
    const fr = from * Math.pow(to / from, 1 - Math.pow(1 - k, 2));
    phase += (2 * Math.PI * fr) / RATE;
    const env = Math.min(1, t / 0.006) * Math.exp(-t / (ms / 1000 * 0.9));
    buf[start + i] += amp * env * (Math.sin(phase) + 0.15 * Math.sin(2 * phase));
  }
}

const SOUNDS = {
  // Onay / soru: iki nota yukarı, "bakar mısın?"
  attention: { seconds: 0.7, notes: [{ at: 0, f: 783.99, decay: 0.12 }, { at: 0.11, f: 1046.5, decay: 0.16 }] },
  // Bitti: hızlı, parlak arpej (Do majör), son nota biraz uzun.
  done: {
    seconds: 0.95,
    notes: [
      { at: 0, f: 523.25, decay: 0.07, amp: 0.8 },
      { at: 0.065, f: 659.25, decay: 0.07, amp: 0.8 },
      { at: 0.13, f: 783.99, decay: 0.08, amp: 0.85 },
      { at: 0.195, f: 1046.5, decay: 0.17 },
    ],
  },
  // Hata: iki nota aşağı, daha alçak ve biraz daha sert ama yine yumuşak.
  error: {
    seconds: 0.8,
    notes: [
      { at: 0, f: 440, decay: 0.1, hard: 0.25, glideFrom: 1.04 },
      { at: 0.14, f: 349.23, decay: 0.16, hard: 0.25, glideFrom: 1.04 },
    ],
  },
  // Zamanlayıcı: "ding-ding … ding-ding", duyulması gereken tek ses; biraz daha uzun.
  timer: {
    seconds: 1.35,
    notes: [
      { at: 0, f: 880, decay: 0.1 },
      { at: 0.12, f: 1318.5, decay: 0.13 },
      { at: 0.5, f: 880, decay: 0.1 },
      { at: 0.62, f: 1318.5, decay: 0.17 },
    ],
  },
  // Açılış selamı: Zody "uyanır" (yükselen bwip) ve iki küçük pırıltı.
  hello: {
    seconds: 1.0,
    sweep: { at: 0, from: 330, to: 740, ms: 120, amp: 0.7 },
    notes: [
      { at: 0.16, f: 1174.66, decay: 0.07, amp: 0.55 },
      { at: 0.24, f: 1567.98, decay: 0.14, amp: 0.6 },
    ],
  },
};

function render({ seconds, notes, sweep: sw }) {
  const buf = new Float64Array(Math.round(seconds * RATE));
  if (sw) sweep(buf, sw);
  for (const n of notes) bell(buf, n);
  // Tepe -4 dBFS; son 30 ms yumuşak kapanış.
  const peak = buf.reduce((m, v) => Math.max(m, Math.abs(v)), 0) || 1;
  const gain = Math.pow(10, -4 / 20) / peak;
  const fade = Math.round(0.03 * RATE);
  for (let i = 0; i < buf.length; i++) {
    const tail = Math.min(1, (buf.length - i) / fade);
    buf[i] *= gain * tail;
  }
  return buf;
}

function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((v, i) => data.writeInt16LE(Math.round(Math.max(-1, Math.min(1, v)) * 32767), i * 2));
  const head = Buffer.alloc(44);
  head.write('RIFF', 0);
  head.writeUInt32LE(36 + data.length, 4);
  head.write('WAVE', 8);
  head.write('fmt ', 12);
  head.writeUInt32LE(16, 16);
  head.writeUInt16LE(1, 20); // PCM
  head.writeUInt16LE(1, 22); // mono
  head.writeUInt32LE(RATE, 24);
  head.writeUInt32LE(RATE * 2, 28);
  head.writeUInt16LE(2, 32);
  head.writeUInt16LE(16, 34);
  head.write('data', 36);
  head.writeUInt32LE(data.length, 40);
  return Buffer.concat([head, data]);
}

mkdirSync(OUT, { recursive: true });
for (const [name, spec] of Object.entries(SOUNDS)) {
  const file = wav(render(spec));
  writeFileSync(join(OUT, `${name}.wav`), file);
  console.log(`${name}.wav  ${(file.length / 1024).toFixed(1)} KB`);
}
