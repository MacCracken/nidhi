# Port Spec 22 — DSP modules: `envelope.rs`, `effect_chain.rs`, `stretch.rs`

Read-only reconnaissance for the Rust → **Cyrius** port of nidhi.
Cyrius model: everything-is-`i64`; heap structs with untyped fields; `#derive(accessors)`
generates `Type_field` / `Type_set_field`; floats are `f64` bit-patterns manipulated by
`f64_add`/`f64_sub`/`f64_mul`/`f64_div`/`f64_lt`/`f64_le`/`f64_gt`/`f64_neg`/`f64_from_i64`/
`i64_from_f64` etc.; errors are **negative integer codes**; no serde / generics / trait objects.

Sources:
- `/home/macro/Repos/nidhi/src/envelope.rs`
- `/home/macro/Repos/nidhi/src/effect_chain.rs`
- `/home/macro/Repos/nidhi/src/stretch.rs`

**All f32 in the Rust source becomes f64 bit-pattern in Cyrius.** Rust does `f32` math; for
parity you may keep f64 throughout (widths only matter for exact bit reproduction, which the
tests below do not require — they use tolerances). Where Rust casts `f32 as u32`/`usize`, that
is a **truncation-toward-zero** conversion (`i64_from_f64` truncates); `.round()` rounds half away
from zero; `.ceil()` rounds up; `.floor()` rounds down. `f64::from(f32)` is a widening no-op here.

---

## Cross-cutting helper (from `lib.rs`)

`flush_denormal(x)` — used by the no_std envelope release ramp:

```rust
pub(crate) fn flush_denormal(x: f32) -> f32 {
    if x.abs() < f32::MIN_POSITIVE { 0.0 } else { x }
}
```

Cyrius: `f64_lt(f64_abs(x), F32_MIN_POSITIVE) ? 0.0 : x`. `f32::MIN_POSITIVE ≈ 1.1754944e-38`
(smallest normal f32). If you stay in f64 use `1.1754943508222875e-38` as the constant, or just
use `f64::MIN_POSITIVE ≈ 2.2250738585072014e-308` — the exact threshold only affects flushing
of vanishingly small values and does not affect any test.

---

# 1. `envelope.rs` — ADSR envelope

The module has **two build modes**. Under `std` (default) `AmpEnvelope` wraps
`naad::envelope::Adsr`. Under `no_std` it uses a built-in linear ADSR. **The two paths implement
the SAME linear-segment math** (verified below — the naad Adsr state machine is nearly identical
to the no_std fallback). **For the Cyrius port, implement the single linear ADSR once** (the
no_std fallback), because Cyrius has no naad. The math is specified precisely below so either path
gives identical results.

## 1.1 `EnvState` enum → integer constants

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EnvState { Idle(default), Attack, Decay, Sustain, Release }
```

Cyrius: `const ENV_IDLE=0; ENV_ATTACK=1; ENV_DECAY=2; ENV_SUSTAIN=3; ENV_RELEASE=4;`
Default = `ENV_IDLE`.

## 1.2 `AdsrConfig` struct — durations in SAMPLES

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[must_use]
pub struct AdsrConfig {
    pub attack_samples: u32,   // attack duration in samples
    pub decay_samples: u32,    // decay duration in samples
    pub sustain_level: f32,    // 0.0–1.0
    pub release_samples: u32,  // release duration in samples
}
```

Cyrius heap struct `AdsrConfig` with `#derive(accessors)`, fields:
`attack_samples` (i64), `decay_samples` (i64), `sustain_level` (f64 bits), `release_samples` (i64).
`u32` → store as non-negative `i64`.

**`Default`**: `attack=0, decay=0, sustain=1.0, release=441` (441 = ~10ms @ 44100).

### `AdsrConfig::from_seconds(attack, decay, sustain, release, sample_rate) -> Self`
All args f32 seconds; `sample_rate` f32. Formulas (note the different `.max` clamps and the
truncating `as u32`):
```
attack_samples  = i64_from_f64( f64_max( attack  * sample_rate , 0.0 ) )   // trunc toward 0
decay_samples   = i64_from_f64( f64_max( decay   * sample_rate , 0.0 ) )
sustain_level   = clamp(sustain, 0.0, 1.0)
release_samples = i64_from_f64( f64_max( release * sample_rate , 1.0 ) )   // NOTE: floor 1.0, not 0.0
```
`clamp(v,lo,hi)` = `v<lo ? lo : (v>hi ? hi : v)`. The `as u32` truncates toward zero AFTER the max.

### `AdsrConfig::to_seconds(&self, sample_rate) -> (f32,f32,f32,f32)`  `#[must_use]`
```
( attack_samples  as f32 / sample_rate,
  decay_samples   as f32 / sample_rate,
  sustain_level,
  release_samples as f32 / sample_rate )
```
Cyrius returns 4 values (struct or 4 out-params): `f64_div(f64_from_i64(attack_samples), sr)`, …
Sustain passes through unchanged.

### `AdsrConfig::is_default_sfz(&self, sample_rate) -> bool`  `#[must_use]`
True when config is "no explicit envelope":
```
attack_samples == 0
 && decay_samples == 0
 && | sustain_level - 1.0 | < f32::EPSILON            // EPSILON ≈ 1.1920929e-7
 && release_samples <= (sample_rate * 0.001) as u32   // ~1ms, truncating cast
```
Cyrius: `abs_diff < 1.1920929e-7` and `release_samples <= i64_from_f64(f64_mul(sr, 0.001))`.

## 1.3 `AmpEnvelope` — per-voice state

std layout wraps `naad::envelope::Adsr`. **Port the no_std layout** (5 fields):

```rust
#[cfg(not(feature="std"))]
pub struct AmpEnvelope {
    config: AdsrConfig,
    state: EnvState,
    level: f32,
    pos: u32,
    release_start_level: f32,
}
```

Cyrius heap struct `AmpEnvelope`, `#derive(accessors)`, fields:
`config` (pointer to AdsrConfig struct — store a copy), `state` (i64), `level` (f64 bits),
`pos` (i64), `release_start_level` (f64 bits).

### `AmpEnvelope::new(config: &AdsrConfig, sample_rate: f32) -> Self`  `#[must_use]`
no_std path ignores `sample_rate` (durations already in samples). Init:
`config = *config` (copy), `state = Idle`, `level = 0.0`, `pos = 0`, `release_start_level = 0.0`.

### `AmpEnvelope::trigger(&mut self)`  (note-on)  `#[inline]`
```
state = Attack; level = 0.0; pos = 0; release_start_level = 0.0;
```

### `AmpEnvelope::release(&mut self)`  (note-off)  `#[inline]`
```
if state != Idle {
    release_start_level = level;   // snapshot current level so release ramps from here
    state = Release;
    pos = 0;
}
```
(No-op if already Idle. Note: does NOT check for already-Release; re-releasing re-snapshots.)

### `AmpEnvelope::tick(&mut self) -> f32`  `#[inline]`
Advances one sample, returns current level. **This is the core state machine** (from
`tick_no_std`). Reproduce EXACTLY:

```
match state:
  Idle:
      level = 0.0

  Attack:
      if attack_samples == 0:
          level = 1.0; state = Decay; pos = 0
      else:
          level = (pos + 1) / attack_samples          // f64: (f64_from_i64(pos)+1)/f64_from_i64(attack_samples)
          pos  += 1
          if pos >= attack_samples:
              level = 1.0; state = Decay; pos = 0

  Decay:
      if decay_samples == 0:
          level = sustain_level; state = Sustain; pos = 0
      else:
          t     = (pos + 1) / decay_samples
          level = 1.0 + (sustain_level - 1.0) * t      // lerp 1.0 → sustain
          pos  += 1
          if pos >= decay_samples:
              level = sustain_level; state = Sustain; pos = 0

  Sustain:
      level = sustain_level

  Release:
      if release_samples == 0:
          level = 0.0; state = Idle; pos = 0
      else:
          progress = (pos + 1) / release_samples
          level    = flush_denormal( release_start_level * (1.0 - progress) )
          pos     += 1
          if level <= 0.0 OR pos >= release_samples:
              level = 0.0; state = Idle; pos = 0

return level
```

**Parity notes** (must match):
- Ramps use `(pos + 1)` in the numerator (1-based), so the FIRST attack tick returns
  `1/attack_samples`, not 0.
- Attack is linear 0→1; Decay lerps 1→sustain; Release lerps `release_start_level`→0.
- The `>= samples` boundary check snaps to the exact endpoint and transitions in the SAME tick.
- Release ends when `level <= 0.0` OR `pos` reaches the end (whichever first).

**naad std-path equivalence** (`naad::envelope::Adsr::next_value`, verified in
`~/.cargo/.../naad-1.0.0/src/envelope.rs`): identical math with two representational differences
that do NOT change observable output for these tests:
1. naad stores stage time as float `stage_samples` and recomputes `attack_samples = attack_time*sr`
   each tick; nidhi's no_std stores integer sample counts directly.
2. naad's Attack uses `current_value = stage_samples / attack_samples` starting at
   `stage_samples=0` (so first tick = 0.0), incrementing AFTER; and transitions on
   `current_value >= 1.0`. nidhi no_std uses `(pos+1)/attack_samples` (first tick = 1/N).
   **This is a real 1-sample phase difference between the two Rust paths**, but the inline tests
   only assert with tolerances (see below). **For the Cyrius port, implement the no_std version**
   (`(pos+1)/N`) — it is the canonical, naad-free algorithm.

### `AmpEnvelope::is_active(&self) -> bool`  `#[inline]`
`state != Idle`.

### `AmpEnvelope::is_releasing(&self) -> bool`  `#[inline]`
`state == Release`.

## 1.4 Envelope inline tests (parity targets; `#[cfg(all(test, feature="std"))]`)
These run against the **std/naad** path in Rust, but the assertions are loose enough that the
no_std algorithm passes too. Port them against the Cyrius (no_std) implementation:

- `adsr_from_seconds`: `from_seconds(0.01,0.05,0.7,0.1,44100)` ⇒ attack=441, decay=2205,
  sustain≈0.7, release=4410. (`0.01*44100=441.0`; `0.05*44100=2205.0`; `0.1*44100=4410.0`.)
- `amp_envelope_trigger_release_cycle`: cfg (a=4,d=4,s=0.5,r=4). Before trigger: `!is_active()`.
  After trigger: `is_active()`. After 100 ticks + 1: level ≈ 0.5 (±0.05, at sustain).
  After `release()` then ≤10000 ticks: `!is_active()`.
- `amp_envelope_attack_ramp`: cfg (a=100,d=0,s=1.0,r=100). `first = tick()`; skip 49; `mid = tick()`
  (the 51st tick). Assert `mid > first` (ramps up).
- `amp_envelope_smooth_release_from_mid_attack`: cfg (a=1000,d=0,s=1.0,r=1000). Tick 500;
  `level_at_release = tick()` (501st) is in (0,1). After `release()`, `first_release = tick()`
  must be `<= level_at_release` (release ramps down from current, not from 1.0). Then converges to
  inactive within 10000 ticks. **This exercises `release_start_level` snapshotting — get it right.**
- `amp_envelope_idle_stays_zero`: default cfg, no trigger ⇒ `tick()==0.0` and `!is_active()`.
- `amp_envelope_zero_attack`: cfg (a=0,d=0,s=0.8,r=100). After trigger + 11 ticks: level ≈ 0.8
  (±0.05) — zero attack and decay jump straight to sustain.

---

# 2. `effect_chain.rs` — per-instrument serial effect chain

Wraps up to `MAX_SLOTS = 5` naad effects in series with per-slot bypass + wet/dry mix.
**Under no_std the chain is a no-op passthrough** (the whole `EffectState` machinery is `std`-only).
**Cyrius has no naad**, so port decision: either (a) implement passthrough-only (matches no_std,
trivially correct, all non-effect tests pass), or (b) port the naad effect DSP too. **Recommended:
port the chain container + mix/bypass logic + passthrough now; stub the 5 effects as identity
until the naad effects themselves are ported.** The container semantics below are the load-bearing
part; the effect DSP lives in the naad port, not here.

## 2.1 Constants & enums
```rust
pub const MAX_SLOTS: usize = 5;
```
`EffectType` (`#[non_exhaustive]`, Default = None):
`const FX_NONE=0; FX_REVERB=1; FX_DELAY=2; FX_CHORUS=3; FX_COMPRESSOR=4; FX_LIMITER=5;`

## 2.2 `EffectSlot` struct
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSlot {
    pub effect_type: EffectType,
    pub bypass: bool,
    pub mix: f32,                 // 0.0 = fully dry, 1.0 = fully wet
    #[cfg(feature="std")] #[serde(skip)] state: EffectState,  // opaque, not serialized
}
```
Cyrius `EffectSlot`, `#derive(accessors)`: `effect_type` (i64), `bypass` (i64 0/1),
`mix` (f64 bits), `state` (pointer to effect object or 0/null for None). `state` is NOT serialized
(`#[serde(skip)]`) — on deserialize it must be rebuilt via `create_state`.

- `EffectSlot::new() -> Self`  `#[must_use]`: `{ effect_type: None, bypass: false, mix: 1.0,
  state: EffectState::None }`.
- `Default` = `new()`.

## 2.3 `EffectState` (std-only) — the naad effect union
```rust
#[cfg(feature="std")] #[derive(Debug, Clone, Default)]
enum EffectState {
    None(default),
    Reverb(Box<naad::reverb::Reverb>),      // boxed (large)
    Delay(naad::delay::CombFilter),
    Chorus(naad::effects::Chorus),
    Compressor(naad::dynamics::Compressor),
    Limiter(naad::dynamics::Limiter),
}
```
In Cyrius, `state` is a tagged pointer: tag in `effect_type`, payload a heap object of the matching
effect type (or null for None). No serde on it.

## 2.4 `EffectChain` struct
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectChain {
    slots: Vec<EffectSlot>,     // private
    sample_rate: f32,           // private
}
```
Cyrius `EffectChain`, `#derive(accessors)`: `slots` (dynamic array pointer), `sample_rate` (f64 bits).

### Methods
- `new(sample_rate: f32) -> Self`  `#[must_use]`: empty slots, store sample_rate.
- `add(&mut self, effect_type) -> bool`: if `slots.len() >= MAX_SLOTS` return `false`. Else build a
  fresh `EffectSlot`, set its `effect_type`, set `state = create_state(effect_type)` (std), push,
  return `true`.
- `remove(&mut self, index: usize)`: if `index < len` remove element at `index` (shifts down).
- `clear(&mut self)`: empties slots.
- `slots(&self) -> &[EffectSlot]`  `#[inline] #[must_use]`.
- `slot_mut(&mut self, index) -> Option<&mut EffectSlot>`  `#[inline] #[must_use]`: bounds-checked
  get_mut → in Cyrius return pointer or 0/null.
- `len(&self) -> usize`  `#[inline] #[must_use]`.
- `is_empty(&self) -> bool`  `#[inline] #[must_use]`.

### `process_sample(&mut self, input: f32) -> f32`  `#[inline]`  — CORE
```
out = input
for each slot in slots (in order):                 // std only; no_std = passthrough
    if slot.bypass OR slot.effect_type == None: continue
    wet = dispatch on slot.state:
        None       -> out
        Reverb(r)  -> r.process_sample(out).0        // takes LEFT channel of (l,r) tuple
        Delay(d)   -> d.process_sample(out)
        Chorus(c)  -> c.process_sample(out)
        Compressor(c) -> c.process_sample(out)
        Limiter(l) -> l.process_sample(out)
    out = out * (1.0 - slot.mix) + wet * slot.mix    // wet/dry blend, in place
return out
```
Cyrius mix: `f64_add( f64_mul(out, f64_sub(1.0, mix)), f64_mul(wet, mix) )`. Effects are applied
**serially** — each slot's output feeds the next. Note the Reverb path discards the right channel.

### `process_stereo(&mut self, left, right) -> (f32,f32)`  `#[inline]`
Runs each channel independently through the whole chain:
`( process_sample(left), process_sample(right) )`. **Both calls mutate the same effect state**, so
left is processed fully, then right — stateful effects (delay/reverb) see left's history. (This is
the source behavior; replicate it, do not "fix" it.)

### `create_state(&self, effect_type) -> EffectState`  (std-only private)
Builds the naad effect from `self.sample_rate` (call it `sr`). **Exact constructor calls (naad
1.0.0 signatures verified) — reproduce arguments EXACTLY when the naad effects are ported:**
```
None       -> EffectState::None
Reverb     -> naad::reverb::Reverb::new(sr, 1.5, 0.3, 0.5, 0.6)  // Result; on Err -> None
                 then Box it: EffectState::Reverb(Box::new(r))
Delay      -> samples = (sr * 0.3) as usize            // 300ms delay, truncating cast
                 EffectState::Delay(naad::delay::CombFilter::new(samples, 0.4))
Chorus     -> naad::effects::Chorus::new(3, 0.02, 0.002, 1.5, 0.5, sr)  // Result; Err -> None
Compressor -> naad::dynamics::Compressor::new(-20.0, 4.0, 0.01, 0.1, sr)  // infallible
Limiter    -> naad::dynamics::Limiter::new(-1.0, 0.05, sr)                 // infallible
```

**IMPORTANT parameter-order caveat (bug-for-bug parity):** naad 1.0.0 signatures are:
- `Reverb::new(decay, damping, pre_delay_ms, mix, sample_rate)` — nidhi passes
  `(sr, 1.5, 0.3, 0.5, 0.6)`, i.e. `sr` lands in `decay`, `1.5` in `damping`, …, and `0.6` in
  `sample_rate`. naad clamps `decay = sr.clamp(0.0,0.99)` and validates `sample_rate=0.6` (>0, ok).
  This is what the source does; if you port the naad Reverb, feed these SAME positional values.
- `Chorus::new(num_voices, base_delay_ms, depth_ms, rate, mix, sample_rate)` — nidhi passes
  `(3, 0.02, 0.002, 1.5, 0.5, sr)` (these line up sensibly).
- `CombFilter::new(delay_samples, feedback)` — `(samples, 0.4)`.
- `Compressor::new(threshold_db, ratio, attack, release, sample_rate)` — `(-20, 4, 0.01, 0.1, sr)`.
- `Limiter::new(ceiling_db, release, sample_rate)` — `(-1.0, 0.05, sr)`.
On `Result` error (Reverb, Chorus) fall back to `EffectState::None` (`.unwrap_or(None)`).

## 2.5 Effect-chain inline tests (`#[cfg(all(test, feature="std"))]`)
- `empty_chain_passthrough`: new chain, `process_sample(0.5) == 0.5`.
- `max_slots_enforced`: add `None` 5× all succeed; 6th `add` returns `false`.
- `bypass_skips_effect`: add Compressor, set slot0.bypass = true, `process_sample(0.5) ≈ 0.5`.
- `remove_slot`: add Reverb + Delay ⇒ len 2; remove(0) ⇒ len 1.
- `wet_dry_mix`: add None, set slot0.mix = 0.0 (fully dry), `process_sample(0.5) ≈ 0.5`.

**Passthrough-port note:** if you port only the container (effects as identity), tests
`empty_chain_passthrough`, `max_slots_enforced`, `remove_slot`, `wet_dry_mix` pass unchanged, and
`bypass_skips_effect` passes trivially. Only exact effect DSP output (not tested here) needs the
naad port.

---

# 3. `stretch.rs` — time-stretching (WSOLA / OLA)

Pure algorithm, **no naad dependency**, `no_std`-safe (`alloc::vec`). Ports cleanly to Cyrius as
f64 array math. Changes duration without changing pitch.

## 3.1 `StretchMode` enum → int constants
`#[non_exhaustive]`, Default = `Wsola`:
`const SM_OLA=0; SM_WSOLA=1; SM_PHASE_VOCODER=2;` Default = `SM_WSOLA`.
`PhaseVocoder` is not implemented and **falls back to WSOLA**.

## 3.2 `TimeStretcher` struct
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct TimeStretcher {
    input: Vec<f32>,      // input samples
    sample_rate: f32,
    frame_size: usize,    // analysis frame, default 1024
    overlap: f32,         // overlap factor, default 0.5 (= 50%)
}
```
Cyrius `TimeStretcher`, `#derive(accessors)`: `input` (f64 array ptr), `sample_rate` (f64),
`frame_size` (i64), `overlap` (f64).

### Constructors / accessors
- `new(input: Vec<f32>, sample_rate: f32) -> Self`: `frame_size = 1024`, `overlap = 0.5`.
- `with_frame_size(mut self, size) -> Self`: sets `frame_size`, returns self (builder).
- `sample_rate() -> f32`, `frame_size() -> usize`, `overlap() -> f32`, `input() -> &[f32]`
  — all trivial accessors (`sample_rate`, `input` are `#[must_use]`).

## 3.3 Free helper functions

### `dot_correlation(a: &[f32], b: &[f32], len: usize) -> f64`  (private, `#[inline]`)
```
n = min(len, a.len(), b.len())
sum = 0.0 (f64)
for i in 0..n: sum += (a[i] as f64) * (b[i] as f64)
return sum
```
Plain dot product over the first `n` elements. Cyrius: accumulate in f64.

### `cross_correlate(a: &[f32], b: &[f32], max_lag: usize) -> isize`  (PUBLIC, `#[inline] #[must_use]`)
Returns the signed lag maximizing dot product of `a` vs `b` shifted by that lag. **Exported API
(used elsewhere / tested), port it.** Algorithm:
```
if a empty OR b empty: return 0
best_lag = 0; best_corr = -inf
// Negative lags: shift a forward
max_neg = min(max_lag, a.len() - 1)          // saturating: if a.len()==0 -> 0
for lag in 1..=max_neg:
    overlap_len = min(a.len() - lag, b.len())    // saturating_sub
    if overlap_len == 0: continue
    sum = Σ_{i=0..overlap_len} a[lag+i]*b[i]      // f64
    if sum > best_corr: best_corr = sum; best_lag = -(lag)
// Non-negative lags: shift b forward
max_pos = min(max_lag, b.len() - 1)          // saturating
for lag in 0..=max_pos:
    overlap_len = min(a.len(), b.len() - lag)     // saturating_sub
    if overlap_len == 0: continue
    sum = Σ_{i=0..overlap_len} a[i]*b[lag+i]       // f64
    if sum > best_corr: best_corr = sum; best_lag = lag
return best_lag
```
Note: lag 0 is evaluated in the second loop (starts at 0). For identical inputs, lag 0 wins (max
autocorrelation) ⇒ returns 0. `isize` result → signed i64 in Cyrius. All saturating subs must not
underflow (clamp to 0).

### `hann_window(size: usize) -> Vec<f32>`  (private, `#[must_use]`)
```
if size == 0: return empty
denom = max(size - 1, 1) as f64
for i in 0..size:
    w = 0.5 * (1.0 - cos(2*PI*i / denom))    // PI = core::f64::consts::PI
    push w as f32
```
Standard periodic-ish Hann (uses `size-1` denominator ⇒ symmetric, endpoints ≈ 0, center ≈ 1).
Cyrius: needs a `cos` (f64). `2*PI ≈ 6.283185307179586`.

### `normalize_by_window_sum(output: &mut [f32], window_sum: &[f32])`  (private)
```
threshold = 1e-6
for (sample, ws) in zip(output, window_sum):
    if ws > threshold: sample /= ws
```
Divides each output sample by accumulated squared-window energy at that position, skipping
positions with negligible energy (avoids div-by-zero). Zips to `min` length.

## 3.4 `stretch(&self, ratio: f64) -> Vec<f32>`  — WSOLA  `#[must_use]`
`ratio > 1.0` = slower/longer; `< 1.0` = faster/shorter; `== 1.0` ≈ identity.

```
// Guard
if input.is_empty() OR frame_size == 0 OR ratio <= 0.0 OR !ratio.is_finite():
    return empty

input_len = input.len()
if input_len < frame_size: return stretch_short(ratio)     // sub-frame path (§3.7)

syn_hop = (frame_size as f64 * (1.0 - overlap as f64)) as usize   // synthesis hop, truncating
if syn_hop == 0: return input.clone()
ana_hop = (syn_hop as f64) / ratio                          // analysis hop (f64, kept fractional)
tolerance = frame_size / 4                                  // integer div — search radius (samples)

window = hann_window(frame_size)

out_len = (input_len as f64 * ratio).ceil() as usize + frame_size
output     = vec![0.0; out_len]     // accumulator
window_sum = vec![0.0; out_len]     // squared-window energy accumulator

prev_frame: Option<Vec<f32>> = None    // previously emitted (unwindowed) frame
frame_idx  = 0

loop:
    out_pos = frame_idx * syn_hop
    if out_pos + frame_size > out_len: break

    expected_input = (frame_idx as f64 * ana_hop) as isize    // truncating; where OLA would read

    // --- WSOLA similarity search around expected_input ---
    if let Some(prev) = prev_frame:
        search_start = max(expected_input - tolerance, 0) as usize
        search_end   = min( (expected_input + tolerance) as usize,
                            input_len.saturating_sub(frame_size) )
        if search_start > search_end:
            optimal_input = max(expected_input, 0) as usize
        else:
            best_pos  = search_start
            best_corr = -inf
            for pos in search_start..=search_end:
                corr = dot_correlation(prev, &input[pos..],
                                       min(frame_size, input_len - pos))
                if corr > best_corr: best_corr = corr; best_pos = pos
            optimal_input = best_pos
    else:
        optimal_input = max(expected_input, 0) as usize

    if optimal_input + frame_size > input_len: break

    // --- window + overlap-add ---
    frame_slice = &input[optimal_input .. optimal_input + frame_size]
    windowed[i] = frame_slice[i] * window[i]   for i in 0..frame_size

    for i in 0..frame_size:
        oi = out_pos + i
        if oi < out_len:
            output[oi]     += windowed[i]
            window_sum[oi] += window[i] * window[i]

    prev_frame = Some(frame_slice.to_vec())    // store UNWINDOWED slice for next correlation
    frame_idx += 1

normalize_by_window_sum(output, window_sum)

target_len = (input_len as f64 * ratio).round() as usize
output.truncate( min(target_len, output.len()) )
return output
```

**WSOLA parity essentials:**
- Output frames are laid at a FIXED synthesis hop (`syn_hop`); input read positions advance at the
  fractional `ana_hop = syn_hop/ratio` but are then *nudged* to the best cross-correlation position
  within ±`tolerance` of `expected_input`.
- Similarity metric = `dot_correlation(prev_frame, input[pos..], min(frame_size, remaining))` —
  correlates the PREVIOUS emitted frame against candidate windows so successive frames splice
  smoothly (waveform continuity).
- `prev_frame` stores the raw (unwindowed) slice actually used, updated every iteration.
- First frame has no `prev_frame` ⇒ uses `expected_input` directly (no search).
- Two break conditions: output would overrun `out_len`, or chosen input frame would overrun input.
- Final normalize by Σ window² then truncate to `round(input_len*ratio)`.

## 3.5 `stretch_ola(&self, ratio: f64) -> Vec<f32>`  — plain OLA  `#[must_use]`
Same guards / setup as WSOLA but **no correlation search** — input position is taken directly:
```
(guards identical; sub-frame -> stretch_short)
syn_hop = (frame_size * (1.0 - overlap)) as usize;  if 0 -> return input.clone()
ana_hop = syn_hop / ratio
window  = hann_window(frame_size)
out_len = (input_len*ratio).ceil() + frame_size
output, window_sum = zeros(out_len)
frame_idx = 0
loop:
    out_pos = frame_idx * syn_hop
    if out_pos + frame_size > out_len: break
    input_pos = (frame_idx * ana_hop) as usize        // NO search — direct
    if input_pos + frame_size > input_len: break
    frame_slice = input[input_pos .. input_pos+frame_size]
    for i in 0..frame_size:
        oi = out_pos + i
        if oi < out_len:
            output[oi]     += frame_slice[i] * window[i]
            window_sum[oi] += window[i]*window[i]
    frame_idx += 1
normalize_by_window_sum(output, window_sum)
target_len = round(input_len*ratio)
output.truncate(min(target_len, output.len()))
return output
```
Difference from WSOLA: `input_pos = (frame_idx*ana_hop) as usize` used directly (no `tolerance`,
no `dot_correlation`, no `prev_frame`). Lower quality, faster.

## 3.6 `stretch_with_mode(&self, ratio: f64, mode: StretchMode) -> Vec<f32>`  `#[must_use]`
Dispatch:
```
Ola                         -> stretch_ola(ratio)
Wsola | PhaseVocoder        -> stretch(ratio)     // PhaseVocoder falls back to WSOLA
```

## 3.7 `stretch_short(&self, ratio: f64) -> Vec<f32>`  (private) — sub-frame resample
For `input_len < frame_size`: linear-interpolation resample by `ratio`.
```
target_len = (input.len() as f64 * ratio).round() as usize
if target_len == 0: return empty
for i in 0..target_len:
    src  = (i as f64) / ratio
    idx  = src.floor() as usize
    frac = (src - idx as f64) as f32
    a = input.get(idx).unwrap_or(0.0)
    b = input.get(idx+1).unwrap_or(a)     // clamp to last sample at the end
    push a + (b - a) * frac
return output
```
Cyrius: `f64_add(a, f64_mul(f64_sub(b,a), frac))`. Out-of-range index reads yield 0.0 (for `a`) or
fall back to `a` (for `b`).

## 3.8 Stretch inline tests (`#[cfg(all(test, feature="std"))]`)
Helper `sine_wave(freq, sr, dur)` builds `len = (sr*dur) as usize` samples of `sin(2π·freq·t)`.
- `stretch_ratio_1_preserves_length`: 0.5s@44100 sine, `stretch(1.0)` length within ±2% of input.
- `stretch_ratio_2_doubles_duration`: `stretch(2.0)` length ≈ 2× input (±5%).
- `stretch_ratio_half_halves_duration`: `stretch(0.5)` length ≈ input/2 (±5%).
- `ola_produces_finite_output`: `stretch_ola(1.5)` all finite, non-empty.
- `wsola_produces_finite_output`: `stretch(1.5)` all finite, non-empty.
- `empty_input_returns_empty`: `stretch(2.0)` and `stretch_ola(2.0)` empty.
- `invalid_ratio_returns_empty`: ratios 0.0, -1.0, NaN, +Inf ⇒ empty (both algos).
- `very_short_input_handled`: input `[0.5,0.3,0.1]` (len 3 < 1024) ⇒ `stretch(2.0)` &
  `stretch_ola(2.0)` non-empty, finite (exercises `stretch_short`).
- `stretch_with_mode_dispatches`: Ola/Wsola/PhaseVocoder all non-empty; `wsola.len() == pv.len()`
  (PhaseVocoder == WSOLA).
- `cross_correlate_finds_zero_lag_for_identical`: `cross_correlate(a, a, 64) == 0`.
- `cross_correlate_empty_returns_zero`: empty arg ⇒ 0.
- `hann_window_shape`: size 256 ⇒ len 256; `w[0]≈0`, `w[255]≈0`, `w[127]≈1.0` (±0.02).
- `with_frame_size_builder`: `new(...).with_frame_size(512).frame_size() == 512`.

---

## Port checklist / gotchas
1. **Envelope**: implement the no_std linear ADSR (`(pos+1)/N` ramps); skip naad wrapper. Snapshot
   `release_start_level` on `release()`. Release ends on `level<=0` OR `pos>=release_samples`.
2. **from_seconds**: release uses `.max(1.0)` floor (min 1 sample); attack/decay use `.max(0.0)`.
   All are truncating `as u32`.
3. **Effect chain**: no_std = passthrough. Port container (add/remove/mix/bypass) now; effects are
   identity until naad effects are ported. Mix: `out*(1-mix) + wet*mix`, applied serially.
   `process_stereo` reuses one stateful chain for L then R (do not parallelize).
4. **Reverb::new arg order is "wrong" in the source** (`sr` in `decay` slot) — replicate verbatim
   if/when the naad Reverb is ported.
5. **Stretch**: pure f64 array math, no deps. WSOLA = OLA + ±(frame_size/4) correlation search
   using `dot_correlation(prev_frame, input[pos..], …)`. `prev_frame` stores the UNWINDOWED slice.
   `PhaseVocoder`→WSOLA. Sub-frame inputs go through `stretch_short` (linear resample).
6. Integer casts of floats **truncate toward zero**; `.round()` is round-half-away; `.ceil()`/
   `.floor()` as named. Saturating subs must clamp at 0 (usize underflow safety).
