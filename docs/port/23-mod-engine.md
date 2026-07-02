# Port Spec — `src/engine.rs` (SamplerEngine core)

Parity-critical core. This is a **read-only** spec for a Rust→Cyrius port. Cyrius model:
everything-is-`i64`, heap structs with untyped fields, `#derive(accessors)` generates
`Type_field`/`Type_set_field`, floats are `f64` bit-patterns manipulated via
`f64_add`/`f64_mul`/`f64_pow`/etc., errors are negative integer codes, no serde / generics /
trait-objects / `Option` / `Vec` (use heap arrays + explicit length + a sentinel/`-1` for
"none"). **All `f32` in the Rust source must become `f64` in Cyrius** (Cyrius has no `f32`);
do the math in `f64` and only truncate where the Rust source does `as u32`/`as usize`/`as
isize` (those are C-style truncation-toward-zero casts).

Source: `/home/macro/Repos/nidhi/src/engine.rs` (1526 lines). This module owns the render
loop. It calls into sibling modules `sample.rs`, `zone.rs`, `instrument.rs`, `envelope.rs`,
`loop_mode.rs`, and (under `std`) into the external `naad` crate. Because Cyrius has no
feature flags, **port the `std` code path** (the default `naad`-backed path). This spec gives
the exact `naad` math inline so you never need naad itself.

---

## 0. Feature-flag decision for the port

The Rust file is `#[cfg]`-split between `std` (uses `naad`) and `no_std` (hand-rolled
fallbacks). **Port the `std` path.** All engine tests are `#[cfg(all(test, feature = "std"))]`
so parity is defined by the `std` path. Where the two paths diverge, the `no_std` fallback is
documented as a footnote only.

The `simd` accumulate/interpolation paths are pure optimizations that produce bit-identical
(modulo f32 rounding) results to the scalar path. **Port the scalar path.**

---

## 1. External `naad` types the engine uses (fully specified inline)

These four naad structs are instantiated per-voice or per-engine. Port them as plain Cyrius
heap structs. All fields are `f64` in Cyrius.

### 1.1 `naad::envelope::Adsr` — wrapped by `AmpEnvelope` (see `envelope.rs` spec 22)

The engine never touches `Adsr` directly; it goes through `AmpEnvelope` (module `envelope.rs`).
For completeness the underlying math (linear ramps, seconds-based):

State enum `EnvelopeState`: `Idle=0, Attack=1, Decay=2, Sustain=3, Release=4`.
Fields: `attack_time, decay_time, sustain_level, release_time, sample_rate` (all f64 seconds/Hz),
plus runtime `state:i64`, `current_value:f64`, `stage_samples:f64`, `release_start_value:f64`.

`gate_on()`: `state = Attack; stage_samples = 0`.
`gate_off()`: `if state != Idle { release_start_value = current_value; state = Release; stage_samples = 0 }`.
`next_value()` per-sample (returns `current_value`):
```
Idle:    current = 0
Attack:  a = attack_time * sr
         if a <= 0 { current = 1; state = Decay; stage = 0 }
         else { current = stage / a; stage += 1; if current >= 1 { current=1; state=Decay; stage=0 } }
Decay:   d = decay_time * sr
         if d <= 0 { current = sustain; state = Sustain }
         else { p = stage/d; current = 1 + (sustain-1)*p; stage += 1;
                if current <= sustain { current = sustain; state = Sustain } }
Sustain: current = sustain
Release: r = release_time * sr
         if r <= 0 { current = 0; state = Idle }
         else { p = stage/r; current = release_start_value*(1-p); stage += 1;
                if current <= 0 { current = 0; state = Idle } }
```
`is_active()` = `state != Idle`. `state()` returns the enum.

**Parity note on `AmpEnvelope::new`**: `AdsrConfig` stores durations in *samples* (u32).
`AmpEnvelope::new` converts config→seconds (`samples/sample_rate`) then hands to naad which
re-multiplies by `sample_rate`. So an `attack_samples=100` config yields naad `attack_time =
100/sr` and naad computes `a = (100/sr)*sr = 100`. Round-trips to the same sample count
(modulo f32 error). Also: naad clamps `a.max(0.0)`, `d.max(0.0)`, `s.clamp(0,1)`, `r.max(0.0)`,
`sample_rate.max(1.0)`, and if construction fails uses fallback `(0,0,1.0,0.01,44100)`.

### 1.2 `naad::filter::StateVariableFilter` — used by `VoiceFilter` (this module)

TPT/Zavalishin SVF, stereo needs **two independent instances** (L and R). Constructor
`new(frequency, q, sample_rate) -> Result`. Errors if `sr` invalid, `frequency` invalid
(>= sr/2 rejected), or `q <= 0`/non-finite. Fields: `frequency, q, sample_rate, g, k, a1,
a2, a3` (coeffs) + runtime state `ic1eq=0, ic2eq=0`.

Coefficient computation (in `new` and `set_params`):
```
g  = tan(PI * frequency / sample_rate)
k  = 1.0 / q
a1 = 1.0 / (1.0 + g*(g + k))
a2 = g * a1
a3 = g * a2
```
`process_sample(input) -> SvfOutput` (returns all 4 outputs):
```
v3 = input - ic2eq
v1 = a1*ic1eq + a2*v3
v2 = ic2eq + a2*ic1eq + a3*v3
ic1eq = flush_denormal(2*v1 - ic1eq)
ic2eq = flush_denormal(2*v2 - ic2eq)
low_pass  = v2
band_pass = v1
high_pass = input - k*v1 - v2
notch     = low_pass + high_pass
```
`set_params(frequency, q) -> Result`: recomputes g,k,a1,a2,a3 (does NOT reset ic1eq/ic2eq).
Errors on invalid freq or q<=0. `q()` accessor returns current q.
`flush_denormal(x)` = `if abs(x) < f32::MIN_POSITIVE (1.175e-38) { 0 } else { x }`.

### 1.3 `naad::modulation::Lfo` — used by `SamplerVoice` (pitch + filter LFOs)

Constructor `Lfo::new(shape, frequency, sample_rate) -> Result`. Engine only ever uses
`LfoShape::Sine`. Errors if sr invalid or `frequency < 0`/non-finite. Initial state:
`phase = 0.0, mode = Bipolar, depth = 1.0`. (S&H seed fields exist but Sine ignores them.)

`next_value()` per-call:
```
raw = sin(phase * TAU)            // TAU = 2*PI, for Sine shape
dt  = frequency / sample_rate
phase += dt
if phase >= 1.0 { phase -= 1.0 }
output = raw                      // Bipolar mode: output == raw
return output * depth             // depth = 1.0
```
So for Sine: **returns `sin(phase*2*PI)` BEFORE advancing phase**, i.e. first call returns
`sin(0)=0`, then phase advances. Range is `[-1, 1]`.

### 1.4 `naad::smoothing::ParamSmoother` — used by `SamplerVoice.cutoff_smoother`

One-pole smoother. Constructor `new(smooth_time_sec, sample_rate, initial_value)`:
```
time  = max(smooth_time, 0)
coeff = if time > 0 { 1 - exp(-1 / (time * sample_rate)) } else { 1.0 }
current = initial_value; target = initial_value
```
Engine constructs with `smooth_time = 0.005`, `initial_value = 0.0`.
`set_target(t)`: `if t is finite { target = t }`.
`next_value()`:
```
current += coeff * (target - current)
current = flush_denormal(current)
return current
```

### 1.5 `naad::voice::VoiceManager` — used by `SamplerEngine.voice_mgr`

**Critical parity subtlety.** `VoiceManager` keeps its OWN internal pool of `naad::voice::Voice`
structs, *parallel to and separate from* the engine's own `Vec<SamplerVoice>`. The engine only
uses `voice_mgr` to **pick an index**; it never renders naad voices. Naad `Voice` fields:
`active:bool, note:u8, velocity:f32, age:u64, amplitude:f32, pitch_bend, pressure, brightness`
(all default 0 / false).

`VoiceManager::new(max_voices, poly_mode, steal_mode)`:
```
n = clamp(max_voices, 1, 128)
voices = n default Voice{active=false, age=0, amplitude=0, note=0, ...}
poly_mode, steal_mode stored as public mutable fields.
```
`note_on(note, velocity: f32) -> Option<usize>`:
```
Mono | Legato: voice[0].{active=true, note, velocity, age=0}; return Some(0)
Poly:
  if some voice !active: pick first free idx; set active/note/velocity/age=0; return Some(idx)
  else: idx = find_steal_target()?; set that voice active/note/velocity/age=0; return Some(idx)
```
`find_steal_target()` (only among `active` voices):
```
None     -> None
Oldest   -> max_by age
Quietest -> min_by amplitude (partial_cmp, Equal on NaN)
Lowest   -> min_by note
```
`note_off(note)`: find first `active && note==note`, set `active=false`, return its idx (else None).
`all_notes_off()`: set every voice `active=false`.
`tick()`: `for active voices: age = age.saturating_add(1)`.

**THE PARITY BUG-COMPATIBLE BEHAVIOR** you must replicate: **the engine NEVER calls
`voice_mgr.tick()` and never updates naad's `amplitude`.** Therefore, inside the naad pool:
- All naad `age` values stay `0` forever.
- All naad `amplitude` values stay `0.0` forever.
- `StealMode::Oldest`: `max_by_key(age)` over all-equal-0 ages ⇒ Rust `max_by_key` returns the
  **last** element among equal keys ⇒ steals the **highest active index**.
- `StealMode::Quietest`: all amplitudes equal 0 ⇒ `min_by` returns the **first** active index.
- `StealMode::Lowest`: genuinely works (uses `note`, which IS set on note_on).
- naad's `note_on` sets its own voice's `active=true`; the engine separately sets the
  corresponding `SamplerVoice.active=true`. They index-align because both pools have the same
  size and both allocate the first free slot / same steal target. Engine's own note_off /
  amp-env expiry set `SamplerVoice.active=false` but do NOT clear the naad voice unless
  `note_off(note)` / `all_notes_off()` is also called on the engine (which forwards to
  voice_mgr). This means the two pools can drift when voices die from envelope completion.

For a faithful port, **model the naad pool explicitly** as a second parallel array of
`{active, note, age, amplitude}` (age/amplitude can be constant-0 since nothing updates them),
OR — simpler and behavior-identical for the tested cases — implement `allocate_voice` to match
the *observable* result: first-free-else-steal where steal uses the rules above with age≡0,
amplitude≡0. See §5 for the exact allocate contract and §9 test `steal_mode_oldest_steals_oldest_voice`,
which passes only because of an implementation detail (see that test's note).

---

## 2. Sibling-module contracts the engine depends on

(Full specs live in their own port docs; summarized here with exact signatures/semantics.)

### `LoopMode` (`loop_mode.rs`) — enum, port as i64 constants
```
OneShot=0 (default), Forward=1, PingPong=2, Reverse=3, LoopSustain=4
```

### `FilterMode` (`zone.rs`)
```
LowPass=0 (default), HighPass=1, BandPass=2, Notch=3
```

### `AdsrConfig` (`envelope.rs`) — struct, all fields public
```
attack_samples: u32   (Cyrius i64)
decay_samples:  u32
sustain_level:  f32   (Cyrius f64)
release_samples:u32
Default = {0, 0, 1.0, 441}
```

### `AmpEnvelope` (`envelope.rs`) — the engine's per-voice envelope
Methods the engine calls: `new(&AdsrConfig, sample_rate) -> Self`, `trigger()` (=gate_on),
`release()` (=gate_off), `tick() -> f32` (=naad next_value), `is_active() -> bool`,
`is_releasing() -> bool` (= naad state == Release). Backed by naad `Adsr` (§1.1).

### `Sample` (`sample.rs`)
- `frames() -> usize`
- `read_stereo_interpolated(position: f64) -> (f32, f32)` — **cubic Hermite / Catmull-Rom**,
  4-tap. Reads frames at `floor(pos)-1, floor(pos), +1, +2`; out-of-range frames read as 0.
  Mono→duplicate to both channels; stereo→deinterleaved. Coeffs:
  ```
  idx  = floor(position)  (as isize)
  frac = position - idx
  y0..y3 = frame(idx-1), frame(idx), frame(idx+1), frame(idx+2)   // 0 if OOB
  a = -0.5*y0 + 1.5*y1 - 1.5*y2 + 0.5*y3
  b =  y0 - 2.5*y1 + 2.0*y2 - 0.5*y3
  c = -0.5*y0 + 0.5*y2
  d =  y1
  out = ((a*t + b)*t + c)*t + d      // Horner, t = frac
  ```
  Apply per channel (L, R). This is the interpolation used for BOTH the main read and the
  crossfade read.

### `SampleBank` (`sample.rs`)
- `get(SampleId) -> Option<&Sample>` — `SampleId(u32)` indexes a `Vec<Sample>`; returns None
  if index OOB. In Cyrius: array + length, return sentinel/-1 for missing.
- `new()`, `add(Sample) -> SampleId`.

### `Instrument` (`instrument.rs`)
- `zones() -> &[Zone]` — the ordered zone array.
- `find_zones(note, velocity) -> Vec<&Zone>` — every zone where `zone.matches(note,vel)`
  (`note in [key_lo,key_hi] && vel in [vel_lo,vel_hi]`), in array order.
- Round-robin (`find_zone_rr`) exists but the ENGINE DOES NOT USE IT — engine uses `find_zones`
  and always takes `zones[0]` (the first match). Do not port RR into the engine path.

### `Zone` (`zone.rs`) — accessors the engine reads (all `#[must_use]`)
The engine reads these in `note_on` and the render loop. Fields are `pub(crate)` so the render
loop reads `zone.loop_start` / `zone.loop_end` as fields directly (not accessors) in a couple
places — note both `zone.loop_start()`/`zone.loop_start` resolve to the same value.
```
sample_id() -> SampleId
playback_ratio(note: u8) -> f64      // = 2^(( (note - root_note) + tune_cents/100 ) / 12)
velocity_curve() -> VelocityCurve ; .apply(vel:u8) -> f32   (see zone spec: Linear/Convex/Concave/Switch)
filter_cutoff() -> f32   (0 = disabled)
filter_resonance() -> f32
filter_type() -> FilterMode
filter_vel_track() -> f32
adsr() -> Option<&AdsrConfig>        // None ⇒ use engine default_adsr
choke_group() -> u32                 // 0 = none
sample_offset() -> usize             // initial playback position
sample_end() -> usize                // 0 = to end of sample
fileg() -> Option<&AdsrConfig>       // filter envelope config
fileg_depth() -> f32                 // cents
pitchlfo_rate() -> f32 ; pitchlfo_depth() -> f32   // Hz / cents
fillfo_rate()  -> f32 ; fillfo_depth()  -> f32
fil_keytrack() -> f32                // 0..1
loop_mode() -> LoopMode
loop_start (field, usize) ; loop_end (field, usize)
crossfade_length() -> usize          // frames, 0 = none
pan() -> f32                         // -1..1
```
`VelocityCurve.apply(vel)` (std path): Linear=`vel/127`; Convex=`sqrt(vel/127)`;
Concave=`(vel/127)^2`; Switch=`if vel>64 {1.0} else {0.0}`.

---

## 3. Public types defined in `engine.rs`

### 3.1 `pub enum PolyMode` — re-exported from naad under std; defined locally under no_std
```
Poly, Mono, Legato    // #[non_exhaustive]
```
Port as i64 constants `POLY=0, MONO=1, LEGATO=2`. Only `Poly` is meaningfully honored by the
engine's own pool (naad handles Mono/Legato in its index picker, but the engine renders one
`SamplerVoice` per allocated index regardless). For a first port, honor Poly.

### 3.2 `pub enum StealMode` — re-exported from naad under std; local under no_std
```
Oldest, Quietest, Lowest, None    // #[non_exhaustive]
```
Port as i64 `OLDEST=0, QUIETEST=1, LOWEST=2, NONE=3`.

### 3.3 `struct VoiceFilter` (PRIVATE — internal to engine, but must be ported)

Per-voice STEREO filter = **two naad SVF instances** (L, R) + a mode. Fields (std path):
```
active: bool
last_cutoff: f32
filter_l: Option<StateVariableFilter>
filter_r: Option<StateVariableFilter>
mode: FilterMode
```
In Cyrius: `active:i64(bool)`, `last_cutoff:f64`, `filter_l_present:i64`, an inlined SVF-L
state (frequency,q,sr,g,k,a1,a2,a3,ic1eq,ic2eq), the same 10 fields for SVF-R, and `mode:i64`.
Or hold two SVF struct pointers + a null sentinel.

Constructors / methods:

**`bypass()`** → `{active=false, last_cutoff=0, filter_l=None, filter_r=None, mode=LowPass}`.
`process_stereo` on a bypassed filter returns input unchanged.

**`new(cutoff, resonance, mode, vel_track, velocity: u8, sample_rate) -> VoiceFilter`**:
```
if cutoff <= 0.0 { return bypass() }
vel_norm = velocity / 127
effective_cutoff = cutoff * (1 - vel_track * (1 - vel_norm))
effective_cutoff = clamp(effective_cutoff, 20.0, sample_rate * 0.49)
q  = max(resonance, 0.1)
fl = StateVariableFilter::new(effective_cutoff, q, sample_rate).ok()   // Option
fr = StateVariableFilter::new(effective_cutoff, q, sample_rate).ok()
return { active = fl.is_some(), last_cutoff = effective_cutoff,
         filter_l = fl, filter_r = fr, mode }
```
Note both L and R are built with the SAME params; `active` follows the L build succeeding.

**`set_cutoff(&mut self, cutoff, sample_rate)`** (per-sample, hot):
```
if !active { return }
cutoff = clamp(cutoff, 20.0, sample_rate*0.49)
if abs(cutoff - last_cutoff) < 0.5 { return }     // dead-band to skip recompute < 0.5 Hz
last_cutoff = cutoff
if filter_l: filter_l.set_params(cutoff, filter_l.q())   // reuse existing q
if filter_r: filter_r.set_params(cutoff, filter_r.q())
```

**`process_stereo(&mut self, left, right) -> (f32, f32)`** (per-sample, hot):
```
if !active { return (left, right) }
l = if filter_l: pick_output(filter_l.process_sample(left),  mode) else left
r = if filter_r: pick_output(filter_r.process_sample(right), mode) else right
return (l, r)
where pick_output(svf_out, mode) = match mode {
    LowPass  => svf_out.low_pass
    HighPass => svf_out.high_pass
    BandPass => svf_out.band_pass
    Notch    => svf_out.notch
}
```

### 3.4 `pub struct SamplerVoice` (PUBLIC — but only 2 accessors are public)

`#[derive(Debug, Clone, Serialize, Deserialize)]`. All fields PRIVATE. Fields (std path), with
Cyrius types:
```
active: bool                      -> i64
zone_index: usize                 -> i64   (index into instrument.zones())
position: f64                     -> f64   (fractional sample-frame read cursor)
speed: f64                        -> f64   (playback rate; frames advanced per output sample)
amplitude: f32                    -> f64   (velocity-curve gain, set at note_on)
note: u8                          -> i64
age: u64                          -> i64   (incremented once per rendered output sample)
forward: bool                     -> i64   (PingPong direction)
amp_env: AmpEnvelope              -> embedded/ptr (naad Adsr state)
filter_env: Option<AmpEnvelope>   -> present flag + embedded Adsr
filter_env_depth: f32             -> f64   (cents)
base_cutoff: f32                  -> f64   (0 ⇒ no filter modulation block runs)
filter: VoiceFilter               -> embedded VoiceFilter
pitch_bend: f32                   -> f64   (semitones, already scaled by pitch_bend_range)
pressure: f32                     -> f64   (0..1)
brightness: f32                   -> f64   (0..1; DEFAULT 0.5)
choke_group: u32                  -> i64   (0 = none)
pitch_lfo: Option<Lfo>            -> present flag + embedded Lfo
pitch_lfo_depth: f32              -> f64   (cents)
filter_lfo: Option<Lfo>           -> present flag + embedded Lfo
filter_lfo_depth: f32             -> f64   (cents)
fil_keytrack: f32                 -> f64   (0..1)
cutoff_smoother: ParamSmoother    -> embedded ParamSmoother (0.005s, sr, init 0.0)
```

**`SamplerVoice::new(sample_rate) -> Self`** — the default/idle voice:
```
active=false, zone_index=0, position=0.0, speed=1.0, amplitude=1.0, note=0, age=0,
forward=true,
amp_env = AmpEnvelope::new(&AdsrConfig::default(), sr),     // default = {0,0,1.0,441}
filter_env=None, filter_env_depth=0.0, base_cutoff=0.0,
filter = VoiceFilter::bypass(),
pitch_bend=0.0, pressure=0.0, brightness=0.5, choke_group=0,
pitch_lfo=None, pitch_lfo_depth=0.0, filter_lfo=None, filter_lfo_depth=0.0,
fil_keytrack=0.0,
cutoff_smoother = ParamSmoother::new(0.005, sr, 0.0)
```

Public accessors (the ONLY public API on the voice):
- `pub fn is_active(&self) -> bool` → returns `active`. `#[inline] #[must_use]`.
- `pub fn note(&self) -> u8` → returns `note`. `#[inline] #[must_use]`.

### 3.5 `pub struct SamplerEngine` (PUBLIC — the main type)

`#[derive(Debug, Clone, Serialize, Deserialize)] #[must_use]`. Fields (std path):
```
voices: Vec<SamplerVoice>         -> heap array + length (length is fixed = max_voices)
instrument: Option<Instrument>    -> present flag + Instrument
bank: SampleBank                  -> embedded
sample_rate: f32                  -> f64
default_adsr: AdsrConfig          -> embedded
pitch_bend_range: f32             -> f64   (semitones; default 2.0)
scratch_buf: Vec<f32>             -> heap f64 array (block render scratch; len grows as needed)
voice_mgr: naad::voice::VoiceManager  -> embedded parallel pool (see §1.5 / §5)
```

---

## 4. `SamplerEngine` public functions (signatures + behavior)

All are `pub`. Names below are exact.

| Fn | Signature | Behavior |
|---|---|---|
| `new` | `new(max_voices: usize, sample_rate: f32) -> Self` | Build `max_voices` idle `SamplerVoice`s (via `SamplerVoice::new`). `instrument=None`, `bank=SampleBank::new()`. `default_adsr = { attack=0, decay=0, sustain_level=1.0, release_samples = max((sr*0.01), 1.0) as u32 }` (~10 ms release). `pitch_bend_range=2.0`. `scratch_buf = 2048 zeros` (1024 stereo frames). `voice_mgr = VoiceManager::new(max_voices, Poly, Oldest)`. |
| `set_instrument` | `(&mut self, Instrument)` | `instrument = Some(i)`. |
| `set_bank` | `(&mut self, SampleBank)` | `bank = b`. |
| `bank` | `(&self) -> &SampleBank` | ref to bank. `#[must_use]`. |
| `bank_mut` | `(&mut self) -> &mut SampleBank` | mut ref. |
| `set_adsr` | `(&mut self, AdsrConfig)` | `default_adsr = a`. |
| `set_release_ms` | `(&mut self, ms: f32)` | `default_adsr.release_samples = max((sr*ms/1000.0), 1.0) as u32`. Only mutates release. |
| `set_pitch_bend_range` | `(&mut self, semitones: f32)` | `pitch_bend_range = max(semitones, 0.0)`. |
| `set_steal_mode` | `(&mut self, StealMode)` | std: `voice_mgr.steal_mode = mode`. |
| `set_poly_mode` | `(&mut self, PolyMode)` | std: `voice_mgr.poly_mode = mode`. |
| `apply_pitch_bend` | `(&mut self, note: u8, bend: f32)` | `bend_semitones = bend * pitch_bend_range`; for every `active` voice with `voice.note==note`, set `voice.pitch_bend = bend_semitones`. |
| `apply_pressure` | `(&mut self, note: u8, pressure: f32)` | for matching active voices set `voice.pressure = clamp(pressure,0,1)`. |
| `apply_brightness` | `(&mut self, note: u8, brightness: f32)` | for matching active voices set `voice.brightness = clamp(brightness,0,1)`. |
| `note_on` | `(&mut self, note: u8, velocity: u8) -> Option<usize>` | See §5. Returns allocated voice index or None. |
| `note_off` | `(&mut self, note: u8)` | std: `voice_mgr.note_off(note)`. Then for every voice with `active && note==note && amp_env.is_active()`: `amp_env.release()` and, if `filter_env` present, `filter_env.release()`. |
| `all_notes_off` | `(&mut self)` | std: `voice_mgr.all_notes_off()`. Then for every voice with `active && amp_env.is_active()`: release amp_env and (if present) filter_env. |
| `next_sample_stereo` | `(&mut self) -> (f32, f32)` | Per-sample render of all voices. See §6. |
| `next_sample` | `(&mut self) -> f32` | `let (l,r) = next_sample_stereo(); (l + r) * 0.5`. |
| `fill_buffer` | `(&mut self, &mut [f32])` | For each element: `*s = next_sample()`. Mono. |
| `fill_buffer_stereo` | `(&mut self, &mut [f32])` | Block render, interleaved stereo. See §7. |
| `fill_buses_stereo` | `(&mut self, &mut [&mut [f32]])` | See §8. |
| `active_voice_count` | `(&self) -> usize` | count of `active` voices. `#[must_use]`. |

Private methods: `allocate_voice` (§5) and `advance_position` (§6.1).

---

## 5. Voice allocation & the choke path — `note_on` (parity-critical)

`note_on(note, velocity) -> Option<usize>`:

**Step 1 — resolve zone (returns None early if no instrument or no matching zone):**
```
instrument = self.instrument or return None
zones_matching = instrument.find_zones(note, velocity)   // Vec<&Zone>, in array order
if zones_matching.is_empty(): return None
zone_idx = index in instrument.zones() of the FIRST matching zone (ptr-eq of zones_matching[0])
           // In practice zone_idx = index of first zone where matches(note,vel).
zone = instrument.zones()[zone_idx]
```
Extract (copies, before any mutable borrow):
```
speed        = zone.playback_ratio(note)             // f64
amp          = zone.velocity_curve().apply(velocity) // f32
f_cutoff     = zone.filter_cutoff()
f_res        = zone.filter_resonance()
f_type       = zone.filter_type()
f_vel        = zone.filter_vel_track()
adsr_config  = zone.adsr().copied().unwrap_or(self.default_adsr)
choke        = zone.choke_group()
sample_offset= zone.sample_offset()
fileg_config = zone.fileg().copied()                 // Option<AdsrConfig>
fileg_depth  = zone.fileg_depth()
plfo_rate    = zone.pitchlfo_rate()   plfo_depth = zone.pitchlfo_depth()
flfo_rate    = zone.fillfo_rate()     flfo_depth = zone.fillfo_depth()
keytrack     = zone.fil_keytrack()
```

**Step 2 — build the voice filter (before allocation):**
```
voice_filter = VoiceFilter::new(f_cutoff, f_res, f_type, f_vel, velocity, self.sample_rate)
```

**Step 3 — choke:** if `choke > 0`, for every voice with `active && choke_group == choke`,
set `voice.active = false` (hard cut, no release). Runs BEFORE allocation, so a choked slot
becomes free and may be reused by this same note_on.

**Step 4 — allocate index:** `voice_idx = allocate_voice(note, velocity)?` (None ⇒ return None,
e.g. StealMode::None with full pool). See allocate contract below.

**Step 5 — initialize the chosen `SamplerVoice`:**
```
voice.active = true
voice.zone_index = zone_idx
voice.position = sample_offset as f64
voice.speed = speed
voice.amplitude = amp
voice.note = note
voice.age = 0
voice.forward = true
voice.filter = voice_filter
voice.base_cutoff = f_cutoff
voice.pitch_bend = 0.0
voice.pressure = 0.0
voice.brightness = 0.5
voice.choke_group = choke

if fileg_config is Some(fc):
    fenv = AmpEnvelope::new(&fc, sr); fenv.trigger()
    voice.filter_env = Some(fenv); voice.filter_env_depth = fileg_depth
else:
    voice.filter_env = None; voice.filter_env_depth = 0.0

// pitch LFO: build only if rate>0 AND depth != 0
voice.pitch_lfo = if plfo_rate > 0.0 && plfo_depth != 0.0
                  { Lfo::new(Sine, plfo_rate, sr).ok() } else { None }
voice.filter_lfo = if flfo_rate > 0.0 && flfo_depth != 0.0
                  { Lfo::new(Sine, flfo_rate, sr).ok() } else { None }
voice.pitch_lfo_depth = plfo_depth
voice.filter_lfo_depth = flfo_depth
voice.fil_keytrack = keytrack

voice.amp_env = AmpEnvelope::new(&adsr_config, sr)
voice.amp_env.trigger()

return Some(voice_idx)
```
Note the amp_env is (re)created fresh and triggered; `cutoff_smoother` is NOT reset on note_on
(it keeps whatever `current` it had — a subtle detail, but it converges to target within a few
samples so is inaudible; replicate anyway: do not reset it in note_on).

**`allocate_voice(note, velocity) -> Option<usize>` (private, std path):**
Delegates to `voice_mgr.note_on(note, velocity as f32 / 127.0)`. Because naad's pool is never
`tick()`ed and its `amplitude` is never updated (see §1.5), the observable behavior is:
```
1. If any naad voice is inactive: return the FIRST inactive index.
   (naad marks it active; engine then also marks the matching SamplerVoice active in step 5.)
2. Else (all naad voices active): steal per steal_mode:
     Oldest   -> highest index (ties on age=0 -> Rust max_by_key returns last)
     Quietest -> lowest index  (ties on amp=0 -> Rust min_by returns first)
     Lowest   -> active voice with smallest naad `note` (note IS tracked)
     None     -> None
```
**IMPORTANT divergence between naad's pool and the engine's own `voices` "free" state:** a naad
voice only becomes inactive again when the engine forwards `note_off`/`all_notes_off` to
voice_mgr. A `SamplerVoice` that dies from envelope completion (amp_env inactive) sets only
`SamplerVoice.active=false`; the parallel naad voice stays active. So over time the naad pool
can be "more full" than the engine pool. In the tested scenarios the pools stay aligned because
tests either (a) don't fill the pool, or (b) fill then immediately steal without note_off. For
a faithful port, replicate the two-pool model: keep a separate `mgr_active[i]`, `mgr_note[i]`
array; `note_on` picks index from THAT array; `note_off(note)`/`all_notes_off` clear it;
envelope death does NOT clear it. (§9 tests only pass with this exact model — see
`steal_mode_oldest_steals_oldest_voice`.)

*(no_std fallback allocate, for reference only: first `!v.active` voice, else per `steal_mode`
over the engine's OWN voices using their real `age`/`amplitude`/`note`; `None ⇒ None`.)*

---

## 6. Per-sample render — `next_sample_stereo` (THE core; parity-critical)

Returns `(out_l, out_r)`. Accumulates all active voices. If `instrument` is None → `(0,0)`.

```
out_l = 0; out_r = 0
zones = instrument.zones()

for voice in voices:
    if !voice.active: continue
    voice.age += 1

    zone = zones[voice.zone_index]
    sample = bank.get(zone.sample_id())      // if None: voice.active=false; continue
    effective_frames = if zone.sample_end() > 0
                       { min(zone.sample_end(), sample.frames()) }
                       else { sample.frames() }

    // --- pitch modulation -> effective_speed ---
    pitch_mod_cents = voice.pitch_bend * 100.0          // semitones->cents, as f64
    if voice.pitch_lfo present:
        pitch_mod_cents += pitch_lfo.next_value() * voice.pitch_lfo_depth   // f64 math
    effective_speed = if pitch_mod_cents != 0.0
                      { voice.speed * 2.0.powf(pitch_mod_cents / 1200.0) }   // f64
                      else { voice.speed }

    // --- read the source, interpolated ---
    (sl, sr) = sample.read_stereo_interpolated(voice.position)     // cubic Hermite, §2

    // --- loop crossfade (only Forward | LoopSustain, only if crossfade_length>0) ---
    xfade = zone.crossfade_length()
    if xfade > 0 && (zone.loop_mode() == Forward || LoopSustain):
        loop_end_f = if zone.loop_end > 0 { zone.loop_end } else { effective_frames }   // as f64
        xfade_f = xfade as f64
        dist_to_end = loop_end_f - voice.position
        if dist_to_end >= 0.0 && dist_to_end < xfade_f:
            t = (dist_to_end / xfade_f) as f32
            xfade_pos = zone.loop_start as f64 + (xfade_f - dist_to_end)
            (xl, xr) = sample.read_stereo_interpolated(xfade_pos)
            sl = sl * t + xl * (1 - t)      // as playback nears loop_end, t->0 => favor loop-start read
            sr = sr * t + xr * (1 - t)

    // --- filter cutoff modulation (only if voice.base_cutoff > 0) ---
    if voice.base_cutoff > 0.0:
        cutoff = voice.base_cutoff
        // key tracking from C4 (note 60)
        if voice.fil_keytrack > 0.0:
            semitones_from_c4 = voice.note - 60          // f32
            keytrack_cents = semitones_from_c4 * 100.0 * voice.fil_keytrack
            cutoff *= 2.0.powf(keytrack_cents / 1200.0)
        // filter envelope (depth in cents)
        if voice.filter_env present:
            env_val = filter_env.tick()
            mod_cents = voice.filter_env_depth * env_val
            cutoff *= 2.0.powf(mod_cents / 1200.0)
        // filter LFO (cents)
        if voice.filter_lfo present:
            lfo_cents = filter_lfo.next_value() * voice.filter_lfo_depth
            cutoff *= 2.0.powf(lfo_cents / 1200.0)
        // brightness scales cutoff over [0.5, 1.0]
        cutoff *= 0.5 + voice.brightness * 0.5
        // one-pole smoothing to avoid clicks
        cutoff_smoother.set_target(cutoff); cutoff = cutoff_smoother.next_value()
        voice.filter.set_cutoff(cutoff, sample_rate)

    // --- apply the stereo filter ---
    (fl, fr) = voice.filter.process_stereo(sl, sr); sl = fl; sr = fr

    // --- amplitude envelope ---
    env = voice.amp_env.tick()
    if !voice.amp_env.is_active(): voice.active=false; continue   // voice died THIS sample -> no output

    // --- pressure & final gain & pan ---
    pressure_mod = 1.0 + (voice.pressure - 0.5) * 0.4     // pressure 0->0.8, 0.5->1.0, 1.0->1.2
    amp = voice.amplitude * env * pressure_mod
    pan = zone.pan()
    pan_l = (1 - pan) * 0.5      // pan=-1 -> L gain 1.0 ; pan=+1 -> L gain 0.0
    pan_r = (1 + pan) * 0.5      // pan=+1 -> R gain 1.0 ; pan=-1 -> R gain 0.0
    out_l += sl * amp * pan_l
    out_r += sr * amp * pan_r

    // --- advance the read cursor using the pitch-bent speed ---
    saved_speed = voice.speed
    voice.speed = effective_speed
    if !advance_position(voice, zone.loop_mode(), zone.loop_start, zone.loop_end,
                         effective_frames, voice.amp_env.is_releasing()):
        voice.active = false
    voice.speed = saved_speed          // restore base speed (bend recomputed each sample)

return (out_l, out_r)
```

**Ordering caveats for parity (do NOT reorder):**
- `age` is incremented at the top of each voice's turn, BEFORE the early-continue on missing
  sample or dead envelope.
- The LFOs are advanced (`next_value()` mutates phase) exactly once per output sample per
  voice, in this order: pitch LFO first (during pitch mod), filter LFO later (during cutoff
  mod). If `base_cutoff <= 0`, the **filter LFO is NOT advanced** and `cutoff_smoother` is NOT
  ticked — replicate that skip.
- `filter_env.tick()` is only called inside the `base_cutoff > 0` block. If base_cutoff is 0
  the filter envelope never advances.
- `amp_env.tick()` is called every sample regardless.
- The voice can be deactivated in TWO places this sample: (a) envelope inactive right after
  `tick()` (before writing output → contributes nothing), (b) `advance_position` returning
  false (AFTER writing output → the current sample IS emitted, then the voice dies).
- `speed` swap: the base `voice.speed` is momentarily overwritten with `effective_speed` for
  the `advance_position` call, then restored. So the stored `speed` is always the un-bent base;
  bend is recomputed from `pitch_bend`+LFO every sample.

### 6.1 `advance_position` (private, `#[inline]`) — loop handling per LoopMode

```
advance_position(voice, loop_mode, loop_start: usize, loop_end: usize,
                 frames: usize, released: bool) -> bool   // false => deactivate voice
    effective_end = if loop_end > 0 { loop_end as f64 } else { frames as f64 }
    match loop_mode:
      OneShot:
        voice.position += voice.speed
        if voice.position >= frames: return false
      Forward:
        voice.position += voice.speed
        if voice.position >= effective_end: voice.position = loop_start as f64
      PingPong:
        if voice.forward:
            voice.position += voice.speed
            if voice.position >= effective_end: voice.forward = false
        else:
            voice.position -= voice.speed
            if voice.position <= loop_start: voice.forward = true
      Reverse:
        voice.position -= voice.speed
        if voice.position < 0.0: return false
      LoopSustain:
        voice.position += voice.speed
        if released:
            if voice.position >= frames: return false        // play out to true end after note-off
        else if voice.position >= effective_end:
            voice.position = loop_start as f64                // loop while held
    return true
```
Notes: `voice.speed` here is the *effective* (pitch-bent) speed (see the swap above). For
Reverse and PingPong-backward, `speed` is still positive and is SUBTRACTED. `effective_end`
uses `loop_end` when nonzero, else the effective frame count. OneShot/Reverse ignore
`loop_start`/`loop_end` for their end test (OneShot uses `frames`, Reverse uses 0).

---

## 7. Block render — `fill_buffer_stereo` (interleaved stereo, per-voice blocking)

Same math as §6 but restructured for cache locality: render each voice across the WHOLE block
into `scratch_buf`, then accumulate into `buffer`. Interleaved stereo: `buffer[2f]=L`,
`buffer[2f+1]=R`.

```
frames = buffer.len() / 2
if frames == 0: return
if instrument is None: buffer.fill(0); return
zones = instrument.zones()
needed = frames * 2
if scratch_buf.len() < needed: scratch_buf.resize(needed, 0.0)   // grow, never shrink
buffer[..needed].fill(0.0)

for vi in 0..voices.len():
    if !voices[vi].active: continue
    scratch_buf[..needed].fill(0.0)                 // clear per-voice scratch
    zone = zones[voices[vi].zone_index]
    sample = bank.get(zone.sample_id())             // None: voices[vi].active=false; continue
    effective_frames = (as §6)
    pan = zone.pan(); pan_l=(1-pan)*0.5; pan_r=(1+pan)*0.5
    xfade = zone.crossfade_length()

    for f in 0..frames:
        voice = &mut voices[vi]
        if !voice.active: break                     // stop filling this voice's scratch
        voice.age += 1
        // ... IDENTICAL per-sample body as §6: pitch mod, read_stereo_interpolated,
        //     crossfade, cutoff modulation (base_cutoff>0), filter.process_stereo,
        //     amp_env.tick + is_active early-break, pressure_mod, amp ...
        scratch_buf[f*2]   = sl * amp * pan_l        // NOTE: '=' assignment, not '+='; scratch was zeroed
        scratch_buf[f*2+1] = sr * amp * pan_r
        // advance_position with speed swap (as §6); on false => voice.active=false (loop continues
        //   to next f but the `if !voice.active: break` at top of next iter stops it)

    accumulate_buffers(&mut buffer[..needed], &scratch_buf[..needed])   // buffer[i] += scratch[i]
```
Parity notes vs. `next_sample_stereo`:
- On dead-envelope this uses `break` (stops rendering the block for this voice) rather than
  `continue`. Since the voice loop is inner, the effect is the same: no further samples for a
  dead voice.
- When the envelope goes inactive mid-block, `voice.active=false` and `break`; the remaining
  scratch frames stay 0 (already zeroed), so nothing is added for those frames. Same for a
  voice that hit end-of-sample: the CURRENT frame's output was written, then next iteration's
  top `if !voice.active: break` stops it.
- The block loop's early-continue on missing sample sets `voices[vi].active=false` (same as §6).
- `accumulate_buffers(dst, src)` = element-wise `dst[i] += src[i]` over `min(len)`.

---

## 8. `fill_buses_stereo` (multi-bus; currently degenerate)

```
if buses.is_empty(): return
frames = buses[0].len() / 2
for frame in 0..frames:
    (out_l, out_r) = next_sample_stereo()
    i = frame*2
    if i+1 < buses[0].len(): buses[0][i] += out_l; buses[0][i+1] += out_r
```
Per-voice bus routing is documented as future work; ALL output currently goes to bus 0 via
`next_sample_stereo`. Higher buses are untouched. Additive (`+=`) into bus 0.

---

## 9. Inline `#[cfg(test)]` tests — port these as the parity acceptance suite

All under `#[cfg(all(test, feature = "std"))]`, i.e. they exercise the naad-backed path.
Helper `make_engine()`: bank with one mono **440 Hz sine, 44100 samples @ 44100 Hz**
(`sine[i] = sin(2π·440·i/44100)`); instrument with one zone `Zone::new(id).with_key_range(0,127)
.with_root_note(69)`; engine `SamplerEngine::new(8, 44100.0)`.

1. **`note_on_produces_output`**: `note_on(69,100)` → `active_voice_count()==1`. Sum
   `|next_sample()|` over 4410 samples → `sum > 0.0`.
2. **`note_off_releases`**: `note_on(69,100); note_off(69)`; run 44100 `next_sample()` →
   `active_voice_count()==0` (release completes and voice deactivates).
3. **`pitch_shift`**: `note_on(81,100)` (root 69, so +12 semitones ⇒ speed=2.0); `fill_buffer`
   4410 → `any(|s| |s|>0.1)`.
4. **`no_instrument_silent`**: fresh `SamplerEngine::new(8,44100.0)` (no instrument):
   `note_on(60,100).is_none()`; `next_sample()==0.0`.
5. **`adsr_envelope_shapes_output`**: `set_adsr({attack=100, decay=100, sustain=0.5,
   release=100})`; `note_on(69,127)`. `first=|next_sample()|`; skip 49; `mid_attack=
   |next_sample()|` → `mid_attack > first` (envelope rising during attack).
6. **`stereo_output_with_pan`**: one zone with `.with_pan(1.0)` (hard right), root 69;
   `note_on(69,127)`; sum `|l|`,`|r|` over 1000 `next_sample_stereo()` → `sum_l < 0.01` AND
   `sum_r > 1.0`. (Confirms pan_l=(1-1)/2·…=0, pan_r=1.)
7. **`filter_reduces_brightness`**: source is a ±1 square (`i%2==0 ? 1 : -1`), 44100 @ 44100.
   engine1 no filter; engine2 zone `.with_filter(100.0, 0.0)` (100 Hz LP cutoff, vel_track 0).
   Both `note_on(69,127)`, sum `|next_sample()|` over 1000 → `sum_filtered < sum_unfiltered`.
   (LP at 100 Hz kills the high-freq square energy.) Engines have max_voices=1.
8. **`fill_buffer_stereo`**: `note_on(69,100)`; `fill_buffer_stereo(&mut [f32;200])` (100
   frames) → `any(|s| |s|>0.01)`.
9. **`all_notes_off_releases_all`**: `note_on(69,100); note_on(72,100)` →count==2;
   `all_notes_off()`; run 44100 samples → count==0.
10. **`per_zone_adsr_overrides_engine_default`**: zone with `.with_adsr({attack=500, decay=0,
    sustain=1.0, release=100})`; `note_on(69,127)`; `first=|next_sample()|`; skip 249;
    `mid=|next_sample()|` → `mid > first` (slow per-zone attack still rising at sample ~250).
11. **`choke_group_silences_previous_voice`**: two zones same sample, keys 42 and 46, BOTH
    `.with_choke_group(1)`. `note_on(46,100)` →count==1; `note_on(42,100)` →count==1 (the open
    hi-hat voice was choked before the closed one allocated).
12. **`pitch_bend_changes_pitch`**: `note_on(69,100)`; 100 samples; `apply_pitch_bend(69,1.0)`
    (full bend, ±2 semitone range ⇒ +2 st); sum 100 `next_sample()` → `sum_bent.is_finite()`
    AND `|sum_bent| > 0.0`.
13. **`steal_mode_none_rejects_when_full`**: `SamplerEngine::new(2,…)`; `set_steal_mode(None)`;
    `note_on(60,100); note_on(64,100)` →count==2; `note_on(67,100).is_none()`; count still 2.
14. **`steal_mode_oldest_steals_oldest_voice`**: `new(2,…)`; `set_steal_mode(Oldest)`;
    `note_on(60,100)`; render 100 samples (ages the FIRST engine voice — but note naad's ages
    stay 0!); `note_on(64,100)` →count==2; `note_on(67,100)` →count==2, and among active engine
    voices the notes contain 67. **Parity detail:** because naad ages are all 0, `Oldest` picks
    the highest index (index 1, currently note 64). So note 64 is stolen, note 60 and 67 remain
    — yet the test only asserts `notes.contains(&67)`, which holds. Your port must replicate the
    two-pool model (naad ages ≡ 0 ⇒ Oldest ⇒ last/highest active index) to keep this AND the
    Lowest test correct.
15. **`steal_mode_lowest_steals_lowest_note`**: `new(2,…)`; `set_steal_mode(Lowest)`;
    `note_on(60,100); note_on(72,100)` →count==2; `note_on(66,100)` →count==2; active notes
    do NOT contain 60, DO contain 66 and 72. (Lowest uses naad `note`, which is tracked, so it
    correctly steals the note-60 voice.)

Tests 14/15 read `engine.voices` directly (private field) — in Cyrius expose a test helper that
lists active `(note)` per voice.

---

## 10. Cyrius porting checklist / gotchas

- **No `f32`.** Everything is `f64`. Truncating casts `as u32`/`as usize`/`as isize` = C
  truncation toward zero; `as u32` on a negative or huge value is UB-ish in Rust but inputs are
  clamped. Do integer floor via a dedicated `f64_floor` for `read_stereo_interpolated`'s
  `position.floor()`.
- **`2.0.powf(x)`** appears 5× in the render (pitch bend, keytrack, filter env, filter LFO,
  and `playback_ratio`'s `2^(semitones/12)`). Use `f64_pow(2.0, x)` (or `f64_exp2`).
- **`Option`** → present-flag `i64` + payload, or `-1` sentinel index. `filter_env`,
  `pitch_lfo`, `filter_lfo`, `instrument`, `bank.get` all return optionals.
- **`Vec<SamplerVoice>`** is fixed-length (=max_voices) after `new`; `scratch_buf` grows
  monotonically. `voices` never resizes.
- **Parallel naad pool** (`voice_mgr`): model it as a small companion array
  `{active[i], note[i]}` (age/amplitude are dead constants). `allocate_voice` reads/writes it;
  `note_off`/`all_notes_off` clear it; envelope-death does NOT. This is required for steal-mode
  parity (§5, §9.14).
- **Denormal flushing** matters in feedback paths (SVF `ic1eq/ic2eq`, smoother `current`,
  no_std envelope). Port `flush_denormal(x) = if abs(x) < 1.175e-38 { 0 } else { x }`.
- **Two SVF instances per voice** for stereo; they share params but have independent state.
- **`set_cutoff` 0.5 Hz dead-band** skips coeff recompute — cheap and audibly identical; keep it.
- **Pan law** is linear (not equal-power): `pan_l=(1-pan)/2`, `pan_r=(1+pan)/2`.
- **Errors**: naad constructors return `Result`; the engine swallows them with `.ok()` /
  `.expect(fallback)`. In Cyrius, a filter/LFO whose construction fails → treat as absent
  (bypass / None). The only `.expect` is the AmpEnvelope fallback `(0,0,1.0,0.01,44100)` which
  is provably valid.
