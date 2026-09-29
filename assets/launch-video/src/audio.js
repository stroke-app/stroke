// Soundtrack: a 150 BPM A-minor bed plus an SFX bus driven by cues.json
// (exported from the page, so every click, key and whoosh lands on its frame).
const fs = require('fs')
const TL = require('./timeline.js')
const CUES = JSON.parse(fs.readFileSync(__dirname + '/cues.json', 'utf8'))

const SR = 48000
const DUR = TL.duration
const N = Math.ceil(DUR * SR)
const BEAT = 0.4, BAR = 1.6
// music bus + reverb send, SFX bus + reverb send
const L = new Float32Array(N), R = new Float32Array(N), sendL = new Float32Array(N), sendR = new Float32Array(N)
const XL = new Float32Array(N), XR = new Float32Array(N), xsL = new Float32Array(N), xsR = new Float32Array(N)

let seed = 11
const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296)
const nz = () => rnd() * 2 - 1
const mtof = m => 440 * Math.pow(2, (m - 69) / 12)
const clamp = (x, a, b) => Math.min(b, Math.max(a, x))
const panLR = p => [Math.cos((p + 1) * Math.PI / 4), Math.sin((p + 1) * Math.PI / 4)]
function add(i, l, r, send = 0) { if (i < 0 || i >= N) return; L[i] += l; R[i] += r; if (send) { sendL[i] += l * send; sendR[i] += r * send } }
function sfx(i, l, r, send = 0) { if (i < 0 || i >= N) return; XL[i] += l; XR[i] += r; if (send) { xsL[i] += l * send; xsR[i] += r * send } }

// ================= music (same bed as v4, new section map) =================
const CHORDS = [{ bass: 45, pad: [57, 60, 64] }, { bass: 41, pad: [57, 60, 65] }, { bass: 48, pad: [55, 60, 64] }, { bass: 43, pad: [55, 59, 62] }]
const chordAt = t => CHORDS[Math.floor(t / BAR) % 4]
const OUTRO = TL.outro[0], BREAK = TL.breakT, LIFT = TL.liftT
function section(t) {
  if (t < TL.hook[1]) return 'hook'
  if (t < TL.logo[1]) return 'reveal'
  if (t >= BREAK[0] && t < BREAK[1]) return 'break'
  if (t < OUTRO) return 'groove'
  return 'outro'
}
const kicks = []
for (let b = 0; b * BEAT < OUTRO; b++) {
  const t = b * BEAT, s = section(t), inBar = b % 4
  if (s === 'hook' && (inBar === 0 || inBar === 2)) kicks.push(t)
  if (s === 'reveal' && inBar === 0 && t >= 4.0) kicks.push(t)
  if (s === 'groove' && (inBar === 0 || inBar === 2 || (inBar === 3 && b % 8 === 7))) kicks.push(t + (inBar === 3 ? BEAT / 2 : 0))
}
kicks.push(TL.hook[1], BREAK[1], OUTRO); kicks.sort((a, b) => a - b)
function duck(t) { let d = 1; for (const k of kicks) { if (k > t) break; const x = t - k; if (x < 0.35) d = Math.min(d, 1 - 0.6 * Math.exp(-x / 0.09)) } return d }
function kick(t0, amp = 0.8) {
  const i0 = Math.round(t0 * SR), len = Math.round(0.35 * SR); let ph = 0
  for (let j = 0; j < len; j++) { const x = j / SR; ph += 2 * Math.PI * (45 + 95 * Math.exp(-x / 0.035)) / SR; const env = Math.exp(-x / 0.12) * Math.min(1, x / 0.002); const v = (Math.tanh(Math.sin(ph) * 1.6) * env + (j < 60 ? nz() * 0.15 * (1 - j / 60) : 0)) * amp; add(i0 + j, v, v) }
}
function noiseHit(t0, { dur, hp = 0, lp = 20000, amp, decay, pan = 0, send = 0, attack = 0.001 }) {
  const i0 = Math.round(t0 * SR), len = Math.round(dur * SR), [gl, gr] = panLR(pan)
  let lpS = 0, hpS = 0, prev = 0; const aL = 1 - Math.exp(-2 * Math.PI * lp / SR), aH = Math.exp(-2 * Math.PI * hp / SR)
  for (let j = 0; j < len; j++) { const x = j / SR; lpS += aL * (nz() - lpS); const h = hp ? aH * (hpS + lpS - prev) : lpS; prev = lpS; hpS = h; const env = Math.min(1, x / attack) * Math.exp(-x / decay); add(i0 + j, h * env * amp * gl, h * env * amp * gr, send) }
}
const clap = t0 => { for (const d of [0, 0.011, 0.022]) noiseHit(t0 + d, { dur: 0.25, hp: 900, lp: 4000, amp: 0.13, decay: d === 0.022 ? 0.09 : 0.012, send: 0.25 }) }
const hat = (t0, amp) => noiseHit(t0, { dur: 0.08, hp: 7000, lp: 16000, amp, decay: 0.018, pan: 0.25, send: 0.05 })
function tone(t0, freq, { dur, amp, attack = 0.005, decay = 0.3, wave = 'sine', pan = 0, send = 0.2, sidechain = false, lp = 0 }) {
  const i0 = Math.round(t0 * SR), len = Math.round(dur * SR), [gl, gr] = panLR(pan); let ph = 0, lpS = 0; const aL = lp ? 1 - Math.exp(-2 * Math.PI * lp / SR) : 1
  for (let j = 0; j < len; j++) {
    const x = j / SR; ph += freq / SR
    let v = wave === 'sine' ? Math.sin(2 * Math.PI * ph) : wave === 'tri' ? 1 - 4 * Math.abs((ph % 1) - 0.5) : 2 * (ph % 1) - 1
    lpS += aL * (v - lpS); v = lp ? lpS : v
    let env = Math.min(1, x / attack) * Math.exp(-x / decay) * Math.min(1, (len - j) / (0.02 * SR)); if (sidechain) env *= duck(t0 + x)
    add(i0 + j, v * env * amp * gl, v * env * amp * gr, send)
  }
}
function padChord(t0, dur, notes, amp) {
  const i0 = Math.round(t0 * SR), len = Math.round(dur * SR), voices = []
  for (const m of notes) for (const c of [-9, 0, 8]) voices.push({ f: mtof(m) * Math.pow(2, c / 1200), ph: rnd(), pan: c / 12 })
  let lpL = 0, lpR = 0
  for (let j = 0; j < len; j++) {
    const x = j / SR, t = t0 + x; let vl = 0, vr = 0
    for (const v of voices) { v.ph += v.f / SR; const s = 2 * (v.ph % 1) - 1; vl += s * (0.5 - v.pan * 0.4); vr += s * (0.5 + v.pan * 0.4) }
    const a = 1 - Math.exp(-2 * Math.PI * (700 + 500 * Math.sin(t * 0.8) ** 2 + (section(t) === 'groove' ? 500 : 0)) / SR)
    lpL += a * (vl - lpL); lpR += a * (vr - lpR)
    const env = Math.min(1, x / 0.25) * Math.min(1, (len - j) / (0.25 * SR)) * duck(t), g = amp / voices.length * 2.2
    add(i0 + j, lpL * env * g, lpR * env * g, 0.35)
  }
}
for (let bar = 0; bar * BAR < OUTRO; bar++) { const t = bar * BAR, s = section(t); padChord(t, BAR + 0.25, chordAt(t).pad, s === 'hook' ? 0.10 : s === 'reveal' || s === 'break' ? 0.16 : 0.13) }
padChord(OUTRO, DUR - OUTRO, CHORDS[0].pad.concat([69]), 0.17)
for (const k of kicks) if (k < OUTRO) kick(k, section(k) === 'reveal' ? 0.6 : 0.75)
kick(OUTRO, 0.8); kick(TL.hook[1], 0.85); kick(BREAK[1], 0.9)
for (let b = 0; b * BEAT < OUTRO; b++) {
  const t = b * BEAT, s = section(t)
  if (s === 'groove' && (b % 4 === 1 || b % 4 === 3)) clap(t)
  if (s === 'hook' || s === 'groove') { hat(t + BEAT / 2, s === 'hook' ? 0.07 : 0.09); if (s === 'groove') hat(t, 0.035) }
}
for (let e = 0; e * BEAT / 2 < OUTRO; e++) {
  const t = e * BEAT / 2, s = section(t)
  if ((s === 'reveal' || s === 'break') && e % 4 !== 0) continue
  const m = chordAt(t).bass
  tone(t, mtof(m - 12), { dur: BEAT / 2 - 0.01, amp: e % 2 ? 0.34 : 0.22, attack: 0.004, decay: 0.16, wave: 'saw', lp: 380, send: 0, sidechain: true })
  tone(t, mtof(m - 12), { dur: BEAT / 2 - 0.01, amp: 0.18, attack: 0.004, decay: 0.2, send: 0, sidechain: true })
}
tone(OUTRO, mtof(33), { dur: 2.8, amp: 0.36, decay: 1.4, send: 0 })
const ARP = [0, 1, 2, 1, 3, 2, 1, 2]
for (let e = 0; e * BEAT / 2 < OUTRO; e++) {
  const t = e * BEAT / 2, s = section(t), ch = chordAt(t).pad, up = t >= LIFT[0] && t < LIFT[1] ? 12 : 0
  const m = [ch[0] + 12 + up, ch[1] + 12 + up, ch[2] + 12, ch[0] + 24][ARP[e % 8]]
  tone(t, mtof(m), { dur: 0.28, amp: s === 'hook' ? 0.07 : s === 'reveal' ? 0.055 : 0.065, attack: 0.002, decay: 0.09, wave: 'tri', pan: e % 2 ? 0.45 : -0.45, send: 0.4 })
}

// ================= SFX engine =================
// RBJ band-pass biquad, coefficients refreshed every 16 samples for sweeps
function bp() { let x1 = 0, x2 = 0, y1 = 0, y2 = 0, b0 = 0, b2 = 0, a1 = 0, a2 = 0
  return { set(f, q) { const w = 2 * Math.PI * clamp(f, 40, 20000) / SR, al = Math.sin(w) / (2 * q), a0 = 1 + al; b0 = al / a0; b2 = -al / a0; a1 = -2 * Math.cos(w) / a0; a2 = (1 - al) / a0 },
    run(x) { const y = b0 * x + b2 * x2 - a1 * y1 - a2 * y2; x2 = x1; x1 = x; y2 = y1; y1 = y; return y } } }
function burst(t0, { f, q = 1.2, dur = 0.03, decay = 0.004, amp, pan = 0, send = 0.05, fEnd = f }) {
  const i0 = Math.round(t0 * SR), len = Math.round(dur * SR), [gl, gr] = panLR(pan), filt = bp()
  for (let j = 0; j < len; j++) { if (j % 16 === 0) filt.set(lerp(f, fEnd, j / len), q); const x = j / SR, v = filt.run(nz()) * Math.min(1, x / 0.0005) * Math.exp(-x / decay) * amp * 3; sfx(i0 + j, v * gl, v * gr, send) }
}
const lerp = (a, b, k) => a + (b - a) * k
function sine(t0, { f, fEnd = f, glide = 0.02, dur, decay, amp, pan = 0, send = 0.1, attack = 0.001 }) {
  const i0 = Math.round(t0 * SR), len = Math.round(dur * SR), [gl, gr] = panLR(pan); let ph = 0
  for (let j = 0; j < len; j++) { const x = j / SR; ph += (fEnd + (f - fEnd) * Math.exp(-x / glide)) / SR; const v = Math.sin(2 * Math.PI * ph) * Math.min(1, x / attack) * Math.exp(-x / decay) * Math.min(1, (len - j) / 96) * amp; sfx(i0 + j, v * gl, v * gr, send) }
}
function sweep(t0, { dur, f0, f1, q = 0.9, amp, pan0 = -0.6, pan1 = 0.6, shape = 'bell', send = 0.25, tail = 0.08 }) {
  const i0 = Math.round(t0 * SR), len = Math.round((dur + tail) * SR), filt = bp()
  for (let j = 0; j < len; j++) {
    const x = j / SR, k = Math.min(1, x / dur)
    if (j % 16 === 0) filt.set(f0 * Math.pow(f1 / f0, k), q)
    const env = x > dur ? Math.exp(-(x - dur) / (tail / 3)) * (shape === 'rise' ? 1 : 0.4) : shape === 'rise' ? k * k : Math.sin(Math.PI * k) ** 1.5
    const [gl, gr] = panLR(lerp(pan0, pan1, k)), v = filt.run(nz()) * env * amp * 2.2
    sfx(i0 + j, v * gl, v * gr, send)
  }
}
const SFX = {
  click(t, a, p) { const f = p ? 1700 : 2300; burst(t, { f, q: 1.6, amp: a * 0.45, decay: 0.0028 }); sine(t, { f: f * 0.42, dur: 0.02, decay: 0.005, amp: a * 0.12 }); burst(t + 0.06, { f: f * 1.15, q: 1.6, amp: a * 0.2, decay: 0.002 }) },
  key(t, a) { const f = 1400 + rnd() * 2000, pan = rnd() * 0.6 - 0.3; burst(t, { f, q: 1.1, amp: a * 0.26, decay: 0.0035, pan }); sine(t, { f: 170 + rnd() * 80, dur: 0.05, decay: 0.016, amp: a * 0.12, pan }) },
  pop(t, a, f = 700) { sine(t, { f: f * 1.9, fEnd: f, glide: 0.012, dur: 0.14, decay: 0.05, amp: a * 0.28, send: 0.2 }); burst(t, { f: f * 3, amp: a * 0.05, decay: 0.002 }) },
  tick(t, a, f = 3500) { sine(t, { f, dur: 0.012, decay: 0.003, amp: a * 0.16 }); burst(t, { f: f * 1.3, q: 2, amp: a * 0.05, decay: 0.0015 }) },
  blip(t, a, f = 1400) { sine(t, { f, dur: 0.09, decay: 0.03, amp: a * 0.16, pan: rnd() * 0.8 - 0.4, send: 0.3 }) },
  open(t, a) { burst(t, { f: 2600, amp: a * 0.12, decay: 0.002 }); sine(t, { f: 480, fEnd: 820, glide: 0.03, dur: 0.12, decay: 0.05, amp: a * 0.14, send: 0.2 }) },
  swish(t, a, dur = 0.3) { sweep(t, { dur, f0: 600, f1: 5200, amp: a * 0.32, q: 0.8 }) },
  whoosh(t, a, dur = 0.62) {
    sweep(t, { dur, f0: 180, f1: 1600, amp: a * 0.5, q: 0.7, shape: 'rise', pan0: -0.7, pan1: 0.7, tail: 0.25 })
    sweep(t, { dur, f0: 1400, f1: 9000, amp: a * 0.22, q: 0.9, shape: 'rise', pan0: -0.7, pan1: 0.7, tail: 0.18 })
  },
  hit(t, a) { sine(t, { f: 95, fEnd: 42, glide: 0.05, dur: 0.45, decay: 0.2, amp: a * 0.55, send: 0.05 }); burst(t, { f: 700, q: 0.7, dur: 0.12, amp: a * 0.22, decay: 0.035 }) },
  slam(t, a) { sweep(t - 0.16, { dur: 0.16, f0: 400, f1: 3500, amp: a * 0.25, shape: 'rise', tail: 0.01 }); SFX.hit(t, a * 1.2); burst(t, { f: 1600, q: 0.9, dur: 0.08, amp: a * 0.3, decay: 0.02 }) },
  riser(t, a, dur = 0.8) { sweep(t, { dur, f0: 300, f1: 6500, amp: a * 0.3, shape: 'rise', q: 1.2, pan0: 0, pan1: 0, tail: 0.04 }); sine(t, { f: 220, fEnd: 880, glide: dur / 2.5, dur, decay: 99, amp: a * 0.04, attack: dur * 0.8 }) },
  scribble(t, a, dur = 0.35) {
    const i0 = Math.round(t * SR), len = Math.round(dur * SR), filt = bp(); filt.set(3200, 1.1)
    for (let j = 0; j < len; j++) { const x = j / SR, k = x / dur, grain = 0.55 + 0.45 * Math.sin(2 * Math.PI * (17 + 5 * Math.sin(x * 9)) * x); const v = filt.run(nz()) * grain * Math.min(1, x / 0.02) * Math.min(1, (1 - k) * 6) * a * 0.5; const [gl, gr] = panLR(lerp(-0.4, 0.4, k)); sfx(i0 + j, v * gl, v * gr, 0.1) }
  },
  success(t, a) { [[88, 0], [93, 0.08], [100, 0.16]].forEach(([m, d]) => { sine(t + d, { f: mtof(m), dur: 1.2, decay: 0.35, amp: a * 0.09, send: 0.5, attack: 0.002 }); sine(t + d, { f: mtof(m) * 2, dur: 0.6, decay: 0.12, amp: a * 0.025, send: 0.5 }) }) },
  shimmer(t, a) { for (let k = 0; k < 9; k++) sine(t + k * 0.03, { f: 2600 + rnd() * 4400, dur: 0.2, decay: 0.05, amp: a * 0.05, pan: rnd() * 1.4 - 0.7, send: 0.6 }) },
  scan(t, a, dur = 0.75) { sine(t, { f: 700, fEnd: 2600, glide: dur / 2, dur, decay: 99, amp: a * 0.05, attack: 0.05, send: 0.3 }); sweep(t, { dur, f0: 800, f1: 6000, amp: a * 0.1, pan0: -0.8, pan1: 0.8 }) },
  whirr(t, a) {
    const i0 = Math.round(t * SR), len = Math.round(0.42 * SR); let ph = 0, lp = 0
    for (let j = 0; j < len; j++) { const x = j / SR; ph += (55 + 160 * Math.min(1, x / 0.3)) / SR; const s = 2 * (ph % 1) - 1; lp += 0.08 * (s - lp); const env = Math.min(1, x / 0.1) * (x < 0.32 ? 1 : Math.exp(-(x - 0.32) / 0.02)); sfx(i0 + j, lp * env * a * 0.22, lp * env * a * 0.22) }
  },
}
for (const [t, type, amp, p] of CUES) { const f = SFX[type]; if (f) f(t, amp, p || undefined) }

// ---------- reverb (Schroeder) ----------
function reverb(inp, combs, aps) {
  const out = new Float32Array(N)
  for (const [d, g] of combs) { const buf = new Float32Array(d); let idx = 0, lp = 0; for (let i = 0; i < N; i++) { const y = buf[idx]; lp = y * 0.7 + lp * 0.3; buf[idx] = inp[i] + lp * g; idx = (idx + 1) % d; out[i] += y / combs.length } }
  for (const [d, g] of aps) { const buf = new Float32Array(d); let idx = 0; for (let i = 0; i < N; i++) { const b = buf[idx], x = out[i], y = -g * x + b; buf[idx] = x + g * y; idx = (idx + 1) % d; out[i] = y } }
  return out
}
const CL = [[1557, 0.84], [1617, 0.83], [1491, 0.85], [1422, 0.84]].map(([d, g]) => [Math.round(d * SR / 44100), g]), CR = [[1580, 0.84], [1640, 0.83], [1514, 0.85], [1445, 0.84]].map(([d, g]) => [Math.round(d * SR / 44100), g])
const AL = [[225, 0.5], [556, 0.5]], AR = [[248, 0.5], [579, 0.5]]
const rvL = reverb(sendL, CL, AL), rvR = reverb(sendR, CR, AR)
const xrL = reverb(xsL, CL.map(([d, g]) => [Math.round(d * 0.7), g * 0.9]), AL), xrR = reverb(xsR, CR.map(([d, g]) => [Math.round(d * 0.7), g * 0.9]), AR)

// ---------- ducking: music makes room under the effects ----------
const W = { whoosh: 1, slam: 1, hit: 0.7, riser: 0.6, success: 0.5, swish: 0.35, click: 0.3, scribble: 0.3, pop: 0.18, key: 0.1 }
const duckEnv = new Float32Array(N)
for (const [t, type, amp, p] of CUES) {
  const w = (W[type] || 0) * amp; if (!w) continue
  const pre = type === 'whoosh' ? (p || 0.62) : type === 'riser' ? (p || 0.8) : 0
  const i0 = Math.round((t) * SR), i1 = Math.round((t + pre + 0.6) * SR)
  for (let i = Math.max(0, i0); i < Math.min(N, i1); i++) { const x = i / SR - t; const v = x < pre ? w * (x / Math.max(pre, 1e-3)) : w * Math.exp(-(x - pre) / 0.22); if (v > duckEnv[i]) duckEnv[i] = v }
}

// ---------- master ----------
let peak = 0
const outL = new Float32Array(N), outR = new Float32Array(N)
for (let i = 0; i < N; i++) {
  const t = i / SR, fadeIn = Math.min(1, t / 0.02), fadeOut = t > DUR - 1.4 ? Math.max(0, (DUR - t) / 1.4) ** 1.5 : 1
  const g = 1 - 0.42 * Math.min(1, duckEnv[i])
  const l = Math.tanh(((L[i] + rvL[i] * 0.55) * g * 0.92 + (XL[i] + xrL[i] * 0.35) * 1.1) * 1.05) * fadeIn * fadeOut
  const r = Math.tanh(((R[i] + rvR[i] * 0.55) * g * 0.92 + (XR[i] + xrR[i] * 0.35) * 1.1) * 1.05) * fadeIn * fadeOut
  outL[i] = l; outR[i] = r; peak = Math.max(peak, Math.abs(l), Math.abs(r))
}
const G = 0.89 / peak, buf = Buffer.alloc(44 + N * 4)
buf.write('RIFF', 0); buf.writeUInt32LE(36 + N * 4, 4); buf.write('WAVE', 8); buf.write('fmt ', 12); buf.writeUInt32LE(16, 16); buf.writeUInt16LE(1, 20); buf.writeUInt16LE(2, 22)
buf.writeUInt32LE(SR, 24); buf.writeUInt32LE(SR * 4, 28); buf.writeUInt16LE(4, 32); buf.writeUInt16LE(16, 34); buf.write('data', 36); buf.writeUInt32LE(N * 4, 40)
for (let i = 0; i < N; i++) { buf.writeInt16LE(Math.round(clamp(outL[i] * G, -1, 1) * 32767), 44 + i * 4); buf.writeInt16LE(Math.round(clamp(outR[i] * G, -1, 1) * 32767), 46 + i * 4) }
fs.writeFileSync(__dirname + '/music.wav', buf)
// stems for checking the balance
function stem(name, A, B) { let pk = 0; for (let i = 0; i < N; i++) pk = Math.max(pk, Math.abs(A[i]), Math.abs(B[i])); const s = Buffer.alloc(44 + N * 4); buf.copy(s, 0, 0, 44); for (let i = 0; i < N; i++) { s.writeInt16LE(Math.round(clamp(A[i] / pk * 0.89, -1, 1) * 32767), 44 + i * 4); s.writeInt16LE(Math.round(clamp(B[i] / pk * 0.89, -1, 1) * 32767), 46 + i * 4) } fs.writeFileSync(__dirname + '/' + name, s) }
stem('stem_sfx.wav', XL, XR)
console.log('music.wav', DUR + 's', 'cues', CUES.length, 'peak', peak.toFixed(3))
