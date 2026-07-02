# Port Spec 20 — Core Modules (`lib.rs`, `error.rs`, `loop_mode.rs`, `zone.rs`)

Rust→Cyrius port spec for nidhi's core types. Read-only recon; no sources modified.

## Cyrius mapping conventions (apply throughout)

- Everything is `i64` on the stack/registers; aggregates are heap structs with untyped fields.
- `#derive(accessors)` generates `Type_field(ptr) -> i64` and `Type_set_field(ptr, val)` per field.
- **Floats** (`f32`/`f64`) are stored as `f64` bit-patterns in an `i64` slot. All arithmetic goes through `f64_add/f64_sub/f64_mul/f64_div/f64_lt/f64_le/f64_gt/f64_ge/f64_eq/f64_abs/f64_max/f64_min/f64_pow/f64_sqrt/f64_from_i64/f64_to_i64`. **nidhi is written in f32 but Cyrius has only f64** — port all `f32` fields as f64 bit-patterns; there is no separate f32 width. Watch the `x.abs() < f32::MIN_POSITIVE` denormal check (see `flush_denormal`): use the f64 smallest-normal constant instead, or drop it (denormals matter only in tight DSP loops, not these config modules).
- **Integers**: `u8` (MIDI notes/vel 0–127), `u32` (groups, SampleId), `usize` (frame indices) all become plain `i64`. There is no unsigned type — clamp/validate ranges manually where Rust relied on `u8` wraparound (nidhi never relies on wraparound here).
- **enums** → integer tag constants (`const LoopMode_OneShot = 0` …). Variants-with-data (only `NidhiError` has them) → tagged heap struct: field 0 = tag, following fields = payload.
- **`Option<T>`** → nullable pointer (0 = `None`). `Zone.adsr` / `Zone.fileg` are `Option<AdsrConfig>` → store 0 or an `AdsrConfig*`.
- **`String`** → heap byte-buffer pointer (nidhi only uses strings in `NidhiError` payloads and instrument names elsewhere).
- **Result<T>** → return the value on success or a **negative error code** on failure. No `Result` struct. See error-code table below.
- No serde/generics/trait-objects: the serde derives and roundtrip tests are **not portable as-is**; reimplement (de)serialization by hand if the port needs persistence, else skip. The `#[cfg(test)]` asserts below still define required behavioral parity.
- `#[must_use]`, `#[inline]`, `#[non_exhaustive]`, `#[default]` are Rust-only annotations with no Cyrius equivalent — noted per type so the porter preserves the *intent* (e.g. `#[default]` variant = the integer the zero-value constructor must use).

---

## `src/lib.rs` — crate root

No public runtime types of its own. Relevant for the port:

- **`pub(crate) fn flush_denormal(x: f32) -> f32`** — `if x.abs() < f32::MIN_POSITIVE { 0.0 } else { x }`. `f32::MIN_POSITIVE` ≈ `1.1754944e-38`. Cyrius: `if f64_lt(f64_abs(x), MIN_NORMAL) then 0.0 else x`. Marked `#[inline] #[must_use]`, `#[allow(dead_code)]` under `std`. Used by hot DSP paths (engine/effects), not by the four modules here.
- `#![cfg_attr(not(feature="std"), no_std)]` + `extern crate alloc` — feature gates: crate builds `no_std`+alloc by default; `std` and `io` are additive features. Cyrius has no feature system; port the **default (std-off) numeric behavior** where it differs (see `VelocityCurve::apply` Convex branch — the no_std Babylonian-sqrt path is the one to replicate for determinism, though it and `f64_sqrt` agree to tolerance).
- **Module list** (all `pub`): capture, effect_chain, engine, envelope, error, instrument, io (behind `io`), loop_mode, sample, sf2, sfz, stretch, zone.
- **`prelude`** re-exports (names the porter's public surface should expose): `SampleRecorder`, `EffectChain`, `EffectType`, `PolyMode`, `SamplerEngine`, `StealMode`, `AdsrConfig`, `AmpEnvelope`, `EnvState`, `NidhiError`, `Result`, `Instrument`, `LoopMode`, `Sample`, `SampleBank`, `SampleId`, `Sf2Preset`, `SfzFile`, `FilterMode`, `VelocityCurve`, `Zone`.
- **`#[cfg(test)] mod assert_traits`**: `public_types_are_send_sync()` asserts `Send+Sync` for every public type (NidhiError, Sample, SampleBank, Sf2Preset, FilterMode, VelocityCurve, Zone, Instrument, SamplerEngine, SampleRecorder, EffectChain, EffectType, PolyMode, StealMode, SamplerVoice, AdsrConfig, AmpEnvelope, EnvState, LoopMode, SampleId, SfzFile, SfzRegion, StretchMode, TimeStretcher). No Cyrius analog (no thread-safety types) — informational only.
- **`#[cfg(all(test, feature="std"))] mod serde_roundtrip`**: `roundtrip<T>(val)` = serialize to JSON then deserialize, expecting success. `all_public_types_roundtrip()` exercises every type; the **maximal `Zone` builder chain** there (lib.rs:161–187) is the single best fixture for `Zone` field parity — reproduce those exact setter calls/values when validating a ported `Zone`. Serde itself is not portable (see conventions).

---

## `src/error.rs`

### `pub enum NidhiError` — `#[non_exhaustive] #[must_use]`, derives `Debug, Clone`

Variants (this is the ONLY variants-with-data type in scope):

| Tag | Variant | Payload |
|----|---------|---------|
| 0 | `SampleNotFound(SampleId)` | one `u32` (the SampleId's inner value) |
| 1 | `InvalidZone(String)` | one string |
| 2 | `InvalidParameter { name: String, reason: String }` | two strings (named struct variant) |
| 3 | `Playback(String)` | one string |
| 4 | `ImportError(String)` | one string |

- `impl Display` — exact format strings (preserve for parity if you emit messages):
  - `SampleNotFound(id)` → `"sample not found: {id.0}"`
  - `InvalidZone(msg)` → `"invalid zone: {msg}"`
  - `InvalidParameter{name,reason}` → `"invalid parameter '{name}': {reason}"`
  - `Playback(msg)` → `"playback error: {msg}"`
  - `ImportError(msg)` → `"import error: {msg}"`
- `#[cfg(feature="std")] impl std::error::Error` (empty).
- **`pub type Result<T> = core::result::Result<T, NidhiError>;`**

**Cyrius strategy.** Two options; pick per call-site richness:
1. Rich errors needed (message survives): heap struct `NidhiError{ tag:i64, a:ptr, b:ptr }` via `#derive(accessors)`, constructed by helpers. Functions return the value or a pointer.
2. Simpler/hot paths: collapse to **negative integer error codes**, e.g. `SAMPLE_NOT_FOUND=-1, INVALID_ZONE=-2, INVALID_PARAMETER=-3, PLAYBACK=-4, IMPORT_ERROR=-5`. `Result<T>` → "non-negative = ok, negative = error code". None of the four modules here actually *return* `Result`/`NidhiError` (all setters return `Self`; getters are infallible), so the code-based approach is sufficient for this spec; the enum only needs a data representation.

---

## `src/loop_mode.rs`

### `pub enum LoopMode` — `#[non_exhaustive]`, `#[default]=OneShot`, derives `Debug,Clone,Copy,Default,PartialEq,Eq,Hash,Serialize,Deserialize`

Fieldless. Integer tags (order = declaration order; the zero-value/default is `OneShot`):

| Tag | Variant | Meaning |
|----|---------|---------|
| 0 | `OneShot` (**default**) | play once, stop |
| 1 | `Forward` | start→end, jump to loop_start, repeat |
| 2 | `PingPong` | forward then backward within loop region |
| 3 | `Reverse` | play backwards |
| 4 | `LoopSustain` | loop while held, play through to end on release |

Cyrius: `const LoopMode_OneShot=0 … LoopMode_LoopSustain=4`. Default constructor returns 0.

---

## `src/zone.rs`

Depends on: `crate::envelope::AdsrConfig`, `crate::loop_mode::LoopMode`, `crate::sample::SampleId`, serde.

**Referenced foreign types (confirmed from sources, needed to port Zone):**
- `SampleId(pub u32)` — newtype over u32; `.0` is the inner id. (`sample.rs:10`)
- `AdsrConfig { pub attack_samples: u32, pub decay_samples: u32, pub sustain_level: f32, pub release_samples: u32 }` (`envelope.rs:25`). `Default` = `{ attack:0, decay:0, sustain:1.0, release:441 }` (441 ≈ 10ms @44.1k). `from_seconds(a,d,s,r,sr)` = `attack=(a*sr).max(0) as u32`, `decay=(d*sr).max(0) as u32`, `sustain=s.clamp(0,1)`, `release=(r*sr).max(1) as u32`.

### `pub enum VelocityCurve` — `#[non_exhaustive]`, `#[default]=Linear`, derives `Debug,Clone,Copy,Default,PartialEq,Eq,Hash,Serialize,Deserialize`

| Tag | Variant | `apply(vel:u8)->f32` (let `v = vel/127.0`) |
|----|---------|--------------------------------------------|
| 0 | `Linear` (**default**) | `v` |
| 1 | `Convex` | `sqrt(v)` — std uses `v.sqrt()`; no_std uses 2-iter Babylonian: `if v<=0 →0` else `x=v; x=0.5*(x+v/x); x=0.5*(x+v/x); x`. Port the Babylonian form with `f64` (converges to `f64_sqrt` within tolerance). |
| 2 | `Concave` | `v*v` |
| 3 | `Switch` | `if vel>64 {1.0} else {0.0}` (branch on raw `vel`, NOT `v`) |

`apply` is `#[inline] #[must_use]`. Note `127.0` divisor; `Switch` threshold is `>64` on the integer velocity.

### `pub enum FilterMode` — `#[non_exhaustive]`, `#[default]=LowPass`, derives `Debug,Clone,Copy,Default,PartialEq,Eq,Hash,Serialize,Deserialize`

| Tag | Variant |
|----|---------|
| 0 | `LowPass` (**default**) |
| 1 | `HighPass` |
| 2 | `BandPass` |
| 3 | `Notch` |

### `pub struct Zone` — `#[must_use]`, derives `Debug,Clone,Serialize,Deserialize`

All fields are `pub(crate)` (module-private; exposed only via getters). **Field order matters** — reproduce for the accessors struct. `#[serde(default)]` / `skip_serializing_if` are serde-only (they define deserialization defaults = the type's zero value; irrelevant without serde). Port each as an `i64` slot (floats as f64 bit-pattern, Options as nullable ptr).

| # | Field | Rust type | `new()` default | Notes |
|---|-------|-----------|-----------------|-------|
| 0 | `sample_id` | `SampleId` (u32) | ctor arg | inner u32 → i64 |
| 1 | `key_lo` | u8 | 0 | MIDI key range lo (incl) |
| 2 | `key_hi` | u8 | 127 | MIDI key range hi (incl) |
| 3 | `vel_lo` | u8 | 1 | vel range lo (incl) |
| 4 | `vel_hi` | u8 | 127 | vel range hi (incl) |
| 5 | `root_note` | u8 | 60 | note of original pitch |
| 6 | `tune_cents` | f32 | 0.0 | fine+transpose, cents |
| 7 | `volume_db` | f32 | 0.0 | dB |
| 8 | `pan` | f32 | 0.0 | -1..+1 |
| 9 | `loop_mode` | LoopMode | `OneShot`(0) | |
| 10 | `loop_start` | usize | 0 | frame; 0 = beginning |
| 11 | `loop_end` | usize | 0 | frame; 0 = end of sample |
| 12 | `crossfade_length` | usize | 0 | frames at loop boundary |
| 13 | `sample_offset` | usize | 0 | playback start frame |
| 14 | `sample_end` | usize | 0 | 0 = end of sample |
| 15 | `filter_cutoff` | f32 | 0.0 | Hz; 0.0 = disabled |
| 16 | `filter_resonance` | f32 | **0.707** | Q; Butterworth default |
| 17 | `filter_type` | FilterMode | `LowPass`(0) | |
| 18 | `filter_vel_track` | f32 | 0.0 | 0..1 |
| 19 | `group` | u32 | 0 | round-robin; 0 = none |
| 20 | `choke_group` | u32 | 0 | 0 = none |
| 21 | `vel_curve` | VelocityCurve | `Linear`(0) | |
| 22 | `adsr` | `Option<AdsrConfig>` | `None`(0) | per-zone amp env |
| 23 | `fileg` | `Option<AdsrConfig>` | `None`(0) | filter env |
| 24 | `fileg_depth` | f32 | 0.0 | cents, ±4800 |
| 25 | `pitchlfo_rate` | f32 | 0.0 | Hz; 0 = disabled |
| 26 | `pitchlfo_depth` | f32 | 0.0 | cents |
| 27 | `fillfo_rate` | f32 | 0.0 | Hz; 0 = disabled |
| 28 | `fillfo_depth` | f32 | 0.0 | cents |
| 29 | `fil_keytrack` | f32 | 0.0 | 0..1 |
| 30 | `time_stretch` | f32 | 0.0 | 0 = disabled; 1=normal |
| 31 | `output_bus` | u8 | 0 | 0 = main, 1+ = aux |

#### Constructor
- `pub fn new(sample_id: SampleId) -> Self` — sets all defaults above.

#### Builder setters (all take `mut self`, return `Self`; consume-and-return pattern). **Clamps are load-bearing — replicate exactly.**

| Signature | Effect / clamp |
|-----------|----------------|
| `with_key_range(lo:u8, hi:u8)` | key_lo=lo, key_hi=hi |
| `with_vel_range(lo:u8, hi:u8)` | vel_lo=lo, vel_hi=hi |
| `with_root_note(note:u8)` | root_note=note |
| `with_tune(cents:f32)` | tune_cents = `cents.clamp(-12800.0, 12800.0)` |
| `with_volume(db:f32)` | volume_db=db (no clamp) |
| `with_pan(pan:f32)` | pan = `pan.clamp(-1.0, 1.0)` |
| `with_loop(mode:LoopMode, start:usize, end:usize)` | loop_mode/start/end |
| `with_crossfade(length:usize)` | crossfade_length=length |
| `with_sample_offset(offset:usize)` | sample_offset=offset |
| `with_sample_end(end:usize)` | sample_end=end |
| `with_filter(cutoff:f32, vel_track:f32)` | filter_cutoff=`cutoff.max(0.0)`, filter_vel_track=`vel_track.clamp(0.0,1.0)` |
| `with_filter_resonance(q:f32)` | filter_resonance = `q.max(0.1)` |
| `with_filter_type(mode:FilterMode)` | filter_type=mode |
| `with_group(group:u32)` | group=group |
| `with_choke_group(group:u32)` | choke_group=group |
| `with_velocity_curve(curve:VelocityCurve)` | vel_curve=curve |
| `with_adsr(config:AdsrConfig)` | adsr = `Some(config)` |
| `with_filter_envelope(config:AdsrConfig, depth_cents:f32)` | fileg=`Some(config)`, fileg_depth=`depth_cents.clamp(-4800.0,4800.0)` |
| `with_pitch_lfo(rate_hz:f32, depth_cents:f32)` | pitchlfo_rate=`rate_hz.max(0.0)`, pitchlfo_depth=depth_cents (no clamp) |
| `with_filter_lfo(rate_hz:f32, depth_cents:f32)` | fillfo_rate=`rate_hz.max(0.0)`, fillfo_depth=depth_cents (no clamp) |
| `with_key_tracking(amount:f32)` | fil_keytrack = `amount.clamp(0.0,1.0)` |
| `with_time_stretch(ratio:f32)` | time_stretch = `ratio.clamp(0.0,4.0)` |
| `with_output_bus(bus:u8)` | output_bus=bus |

#### Getters (all `#[inline] #[must_use]`, `&self`, no clamp)
`crossfade_length()->usize`, `sample_offset()->usize`, `sample_end()->usize`, `choke_group()->u32`, `velocity_curve()->VelocityCurve`, `adsr()->Option<&AdsrConfig>`, `fileg()->Option<&AdsrConfig>`, `fileg_depth()->f32`, `pitchlfo_rate()->f32`, `pitchlfo_depth()->f32`, `fillfo_rate()->f32`, `fillfo_depth()->f32`, `fil_keytrack()->f32`, `time_stretch()->f32`, `output_bus()->u8`, `group()->u32`, `filter_cutoff()->f32`, `filter_resonance()->f32`, `filter_type()->FilterMode`, `filter_vel_track()->f32`, `pan()->f32`, `sample_id()->SampleId` (`#[must_use = "returns the sample ID for this zone"]`), `loop_mode()->LoopMode`. → In Cyrius these are the `#derive(accessors)` `Zone_field` reads.

#### Behavioral methods (the real logic to port precisely)

- **`pub fn matches(&self, note:u8, velocity:u8) -> bool`** `#[inline] #[must_use]`:
  `note>=key_lo && note<=key_hi && velocity>=vel_lo && velocity<=vel_hi` (all inclusive).

- **`pub fn playback_ratio(&self, note:u8) -> f64`** `#[inline] #[must_use]`:
  ```
  semitones = (note as f64 - root_note as f64) + (tune_cents as f64 / 100.0)
  ratio     = 2.0_f64.powf(semitones / 12.0)
  ```
  Cyrius: `semitones = f64_add(f64_sub(f64_from_i64(note), f64_from_i64(root_note)), f64_div(tune_cents, 100.0))`; `ratio = f64_pow(2.0, f64_div(semitones, 12.0))`. Returns f64 bit-pattern. This is the pitch/resampling ratio — 1.0 at root, ×2 per octave.

### `#[cfg(all(test, feature="std"))] mod tests` — parity assertions

1. **`zone_matches`**: Zone `key_range(60,72) vel_range(1,127)` ⇒ `matches(66,100)==true`, `matches(59,100)==false`, `matches(73,100)==false`.
2. **`playback_ratio_root`**: `root_note(60)` ⇒ `|playback_ratio(60) - 1.0| < 0.001`.
3. **`playback_ratio_octave_up`**: `root_note(60)` ⇒ `|playback_ratio(72) - 2.0| < 0.01`.
4. **`playback_ratio_with_tuning`**: `root_note(60).with_tune(50.0)` ⇒ `playback_ratio(60)` is `>1.0` and `<1.06` (50 cents = half a semitone; 2^(0.5/12)≈1.0293).

(Not in this file but the lib.rs serde test's maximal `Zone` builder is the recommended full-field construction fixture.)

---

## Port checklist / gotchas

- **f32→f64 only**: no f32 in Cyrius; `filter_resonance` default `0.707`, denormal threshold, and all clamps become f64 constants.
- **Clamps are the logic** — every `with_*` clamp above must be ported; skipping them changes serialized/roundtrip parity.
- **`Switch` velocity curve** branches on raw integer `vel > 64`, all others on `v = vel/127.0`.
- **`0` is overloaded as "disabled/end-of-sample"** for loop_start/loop_end/sample_end/filter_cutoff/lfo rates/time_stretch — preserve that sentinel meaning; downstream engine code depends on it.
- **`Option` fields** (`adsr`, `fileg`) → null-pointer (0) for None; a non-null `AdsrConfig*` for Some.
- **Enum defaults** the zero-constructor must use: `LoopMode=0(OneShot)`, `VelocityCurve=0(Linear)`, `FilterMode=0(LowPass)`.
- **No `Result` returned** by any fn in these four files — all setters return the struct, all getters are infallible. The `NidhiError`/`Result` alias only needs a *representation* here, exercised by other modules.
