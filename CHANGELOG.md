# Changelog

## 2.0.1 — 2026-08-31

Toolchain and dependency catch-up. **No functional change to nidhi's own behaviour** — the only
source edits are five renames forced by naad's namespace migration.

### Changed
- **Cyrius toolchain 6.3.36 → 6.5.36** (`cyrius.cyml [package].cyrius`, the single source of
  truth; CI reads the pin rather than hardcoding it)
- **naad 2.1.0 → 2.2.2**, **shravan 2.5.12 → 2.8.0**, **hisab 2.6.7 → 2.11.2**. Transitive:
  goonj 2.0.0 → 2.0.4, sankoch 1.0.0 → 2.7.10, sakshi 2.4.2 → 2.4.11.
- **naad namespace migration** — naad 2.1.1/2.2.0 moved six bare-lowercase helpers onto the
  `naad_` prefix. nidhi's five call sites follow; the function bodies are byte-identical, so
  output is bit-stable:
  ```
  normalize         -> naad_normalize          src/capture.cyr:136
  rms               -> naad_rms                src/capture.cyr:144
  peak              -> naad_peak               tests/capture.tcyr:81,128
  rms               -> naad_rms                tests/capture.tcyr:88
  db_to_amplitude   -> naad_db_to_amplitude    tests/naad_link.tcyr:13
  amplitude_to_db   -> naad_amplitude_to_db    tests/naad_link.tcyr:14
  ```
- `bump-version.sh` no longer edits a `Cargo.toml` — the file has not existed since the Cyrius
  port, and `set -e` made the script abort on it. VERSION is the sole source of truth
  (`cyrius.cyml` derives it via `${file:VERSION}`); the script now validates the SemVer argument
  and warns if that derivation is ever inlined.
- `docs/development/state.md` refreshed — it still described the 0.1.0 `cyrius port` scaffold.

### Fixed
- **Three undefined symbols resolved**: `mutex_new` / `mutex_lock` / `mutex_unlock` were
  referenced by the vendored bundles with nothing supplying them. The 6.5.36 stdlib snapshot adds
  `lib/sync.cyr` (plus `thread.cyr`, `thread_local.cyr`, `mmap.cyr`, `callback.cyr`), so the
  build now has **zero** undefined-function warnings, down from three.

### Not changed, and why — naad's headline rename does not apply
naad 2.2.0's `FILTER_* → NAAD_FILTER_*` rename names nidhi explicitly, because nidhi defines
`FILTER_LOWPASS..FILTER_NOTCH = 0..3` with values identical to naad's. **naad renamed its own
constants to de-collide, so nidhi needs no edit** — nidhi never consumed naad's `FILTER_*`, and
`src/zone.cyr:30-33` keeps its definitions. Likewise `ERR_* → NAAD_ERR_*` (2.1.3) and
`VOICE_* → NAAD_VOICE_*`: nidhi's `ERR_*` in `src/error.cyr` are its own.

nidhi references none of naad's behaviour-changed surfaces — `convolution`, `fit_polynomial`,
`tuning_note_name`, `ambisonics`, `binaural`, `panning`, `chromagram` — and none of hisab's public
API. Every other naad function nidhi calls is bit-identical between 2.1.0 and 2.2.2, and struct
layouts and enum values are unchanged, so the derive-generated accessors are safe.

### Quality
- **14 suites / 313 assertions / 0 failures**, identical per-suite counts to 2.0.0
- **Fuzz**: 2 harnesses, 0 crashes · **`cyrius audit`** exits 0 · `dist/nidhi.cyr` restamped v2.0.1
- **Benchmarks** re-run on 6.5.36 (`bench-history.csv`, `BENCHMARKS.md`). Everything is within
  run-to-run noise of the 6.3.34 baseline except `fill_buffer_stereo_filtered_8v`, **~12–13 %
  faster** across three runs (2.247 / 2.289 / 2.313 ms vs 2.592 ms) — the path that leans hardest
  on naad's SVF. Note that 6.5.19 reworked `lib/bench.cyr` to subtract a calibrated clock-read
  floor from every sample; at nidhi's batch sizes that is ≤0.3 ns/op, so the two series remain
  comparable, but sub-200 ns rows are dominated by timer jitter and should be read as indicative.

### Known — pre-existing, not introduced here
- **Integer-PCM WAVs decode to silence.** nidhi never calls `shravan_init_constants()`, so
  shravan's `F64_RCP_128` / `F64_RCP_32768` / `F64_RCP_8388608` / `F64_RCP_2147483648` stay at
  their `0` initialiser — which as an f64 bit pattern is `+0.0`. Every 8/16/24/32-bit integer PCM
  file therefore scales to zero; only `PCM_F32` decodes correctly. `tests/io.tcyr` does not catch
  it because every fixture encodes `PCM_F32`. Verified identical in shravan 2.5.12 and 2.8.0, so
  this predates the upgrade — but it is a real defect and should be fixed on its own, with
  integer-PCM coverage added to `tests/io.tcyr`.
- **`ERR_NONE` collides three ways** — `src/error.cyr:18`, `lib/goonj.cyr:16`, and
  `lib/shravan.cyr:82` (`enum ShravanErr`). All three are `0`, so nothing misbehaves, and Cyrius
  warns on a duplicate `fn` but not a duplicate `var`. This is exactly the invisible-until-one-
  side-renumbers hazard naad escaped in 2.2.0. Renaming nidhi's public error constants to the
  `N_` convention is breaking, so it is deferred to the next minor.
- **`.github/workflows/ci.yml` builds `src/main.cyr`**, a scaffold path that does not exist — the
  entry is `programs/smoke.cyr`.
- **Release notes never extract a changelog body.** `.github/workflows/release.yml:61` awks for
  `^## \[$TAG\]` — a *bracketed* heading — but no heading in this file has ever used brackets, so
  every release has silently fallen back to "No changelog entry for $TAG." Bracketing only this
  entry would be worse, not better: with no later bracketed heading the awk never hits its exit
  and would paste the entire file into the 2.0.1 release body (verified: 216 lines). The real fix
  is the awk pattern, not the headings.

## 2.0.0 — 2026-07-03

Full Rust → **Cyrius** port (toolchain 6.3.36). Major version marking the rewrite (and aligning
with the naad/shravan/hisab 2.x ecosystem); supersedes the Rust 1.1.0 line below. The Rust 1.1.0 source is preserved under
`rust-old/` as the parity oracle. All 14 modules ported for **feature-set parity**, leaning on
the converted **naad** (DSP/filters/LFOs/effects/voice management), **shravan** (WAV codecs), and
**hisab** (math) Cyrius libraries.

### Ported
- `error` (integer codes), `f64_util`, `loop_mode`, `envelope` (naad ADSR), `zone` (32 fields +
  velocity curves + `matches`/`playback_ratio`), `sample` (cubic-hermite interp, energy onset
  detection, `SampleBank`), `instrument` (find_zones + round-robin), `capture` (recorder, trim,
  loop detection), `stretch` (WSOLA/OLA), `effect_chain` (5-slot naad effects), `io` (WAV
  load/stream via shravan), `sf2` (RIFF/SoundFont binary parser), `sfz` (40+ opcode text parser),
  `engine` (voice mgmt + per-sample render loop over naad SVF/LFO/smoother/VoiceManager)
- Samples/floats are f64 (Cyrius has no f32); `#derive(Serialize)`-ready config structs; symbols
  `n_`/`N`-prefixed for the flat bundle namespace

### Quality
- **14 test suites, ~327 assertions, 0 failures** (`cyrius test`)
- Adversarial parity audit vs `rust-old/` (2 passes): fixed a sub-1.0-sample-rate envelope
  divergence, SFZ integer-parse strictness (u8 `>255` / negative-unsigned / leading `+`), an SF2
  malformed-sub-chunk error path, and made capture's loop-point sort stable
- **Benchmarks** — 7 Criterion benchmarks reproduced in `tests/nidhi.bcyr` (`BENCHMARKS.md`,
  `bench-history.csv`) for Rust-vs-Cyrius comparison
- **Fuzz** — `fuzz/fuzz_sf2.fcyr` + `fuzz/fuzz_sfz.fcyr` never-crash harnesses (6000 mutated/random
  inputs, 0 crashes)
- `dist/nidhi.cyr` distributable bundle via `cyrius distlib`

## 1.1.0 — 2026-03-28

Performance + real-time safety release. Zero-allocation render path, block-based voice rendering, filter caching, denormal protection, and SIMD infrastructure.

### Performance
- **Block-based voice rendering** — `fill_buffer_stereo` now renders each voice for the entire block into a pre-allocated scratch buffer, then accumulates into output. ~2.9x speedup for single-voice workloads, ~1.2x for 16 voices
- **Filter coefficient caching** — epsilon check on cutoff skips expensive `set_params()` when cutoff hasn't changed meaningfully (< 0.5 Hz). 3.4x speedup on filtered voices
- **Parameter smoothing** — per-voice `naad::smoothing::ParamSmoother` on filter cutoff modulation for click-free filter changes (std only)
- **SIMD stereo mixing** — SSE2 (x86_64) and NEON (aarch64) buffer accumulation behind `simd` feature gate, with scalar fallback
- **SIMD cubic Hermite interpolation** — SSE-accelerated stereo interpolation computes both L/R channels in a single SIMD pass (behind `simd` feature gate)
- **Pre-allocated scratch buffer** — engine allocates a reusable stereo scratch buffer at construction, eliminating render-path heap allocation

### Real-time Safety
- **Denormal flushing** — `flush_denormal()` applied to no_std filter feedback paths and envelope release ramp to prevent 10–100x slowdowns on x86
- **Removed per-sample Vec allocation** in `fill_buses_stereo` — eliminated `Vec::new()` that was called every sample frame

### Bug Fixes
- **Fixed infinite loop** in `detect_onsets()` when sample has ≤ 3 frames (hop became 0)
- **Fixed integer overflow** in SF2 chunk iterator — crafted SF2 with large chunk size could cause wraparound and infinite loop
- **Fixed `stretch()`/`stretch_ola()`** — ratio ≤ 0, NaN, or infinity now returns empty instead of producing inf/NaN

### Quality
- **Benchmark suite** — 7 Criterion benchmarks: voice count scaling, block vs per-sample buffer fill, cubic/stereo interpolation, filtered rendering, WSOLA throughput
- Added `#[must_use]` on 10 accessors/constructors across 5 modules
- Added `#[inline]` on 9 hot-path render functions and accessors
- **117 unit tests + 4 doc-tests** (up from 114)
- New `simd` feature flag for SIMD-accelerated mixing and interpolation

## 1.0.1 — 2026-03-28

### Changed
- **Replace hound with shravan** for WAV I/O — shravan provides broader codec support (WAV, FLAC, AIFF, Ogg, MP3, Opus), streaming decoding, and PCM format conversion
- `StreamingWavReader` now uses shravan's `WavStreamDecoder` for chunked decoding

## 1.0.0 — 2026-03-28

Stable release. Full-featured sample playback engine for AGNOS.

### Engine
- **Polyphonic playback** with configurable voice count and cubic Hermite interpolation
- **Voice management** via `naad::VoiceManager` (std) with hand-rolled fallback (no_std)
- **Steal modes**: Oldest, Quietest, Lowest, None (`StealMode` enum)
- **Poly modes**: Poly, Mono, Legato (`PolyMode` enum)
- **Choke groups**: Voices in the same group silence each other on note-on
- **Per-note expression**: `apply_pitch_bend()`, `apply_pressure()`, `apply_brightness()`
- **Pitch bend range** config (default ±2 semitones)
- **Multi-output routing**: Per-zone bus assignment, `fill_buses_stereo()`

### Zones
- **Key/velocity mapping** with full MIDI range, round-robin groups
- **Root note + tuning** (cents, transpose support)
- **Volume, pan** (constant-power stereo)
- **Velocity curves**: Linear, Convex, Concave, Switch
- **Filter**: SVF (LP/HP/BP/Notch) via naad with true stereo, resonance, velocity tracking, key tracking
- **Filter envelope**: Per-zone `fileg_*` config, modulates cutoff per-sample
- **Per-zone ADSR**: Overrides engine default, wired from SFZ `ampeg_*` opcodes
- **Pitch LFO + Filter LFO**: Per-voice via naad, from zone config
- **Loop modes**: OneShot, Forward, PingPong, Reverse, LoopSustain (release exits loop)
- **Crossfade loops**: Configurable linear blend at loop boundary
- **Sample offset/end**: Partial playback within a sample
- **Time-stretch ratio**: Per-zone config (0.25x–4.0x)
- **Output bus**: Per-zone routing to main or aux buses

### Envelopes
- **AmpEnvelope**: Wraps `naad::envelope::Adsr` (std) or built-in linear ADSR (no_std)
- **Smooth release** from any envelope level
- **AdsrConfig**: Sample-based config with `from_seconds()` convenience

### SFZ Import
- **Parser**: `<global>`, `<group>`, `<region>`, `<control>`, `<curve>` headers
- **40+ opcodes**: sample, key ranges, velocity, pitch_keycenter, tune, transpose, volume, pan, loop modes, filter (cutoff, resonance, fil_type, fil_veltrack), envelopes (ampeg_*, fileg_*), LFOs (pitchlfo_*, fillfo_*), fil_keytrack, offset, end, output
- **Note-name parsing**: C-1 through G9 with sharps/flats
- **`key` shorthand**: Sets lokey=hikey=pitch_keycenter
- **`<control> default_path`**: Prepends path to all sample filenames
- **SFZ v2**: `#include` directive collection, `_onccN` CC modulation parsing
- **Inheritance**: Global → group → region with correct override semantics

### SF2/SoundFont Import
- **RIFF binary parser**: Pure `&[u8]` parsing, no_std compatible
- **Preset/instrument/zone chain** resolution with key/velocity range masking
- **PCM16→f32** sample data extraction
- **Loop mode mapping**: SF2 sampleModes → nidhi LoopMode (including mode 3 → LoopSustain)
- **Returns nidhi-native types**: `(Vec<Sf2Preset>, Vec<Instrument>, SampleBank)`

### Sample Capture
- **SampleRecorder**: Accumulate `&[f32]` audio buffers into a `Sample`
- **Auto-trim**: `trim_silence()` removes leading/trailing silence
- **Normalize**: `normalize_peak()` (0 dB) and `normalize_rms()` (target RMS)
- **Loop detection**: `detect_loop_points()` via zero-crossing + cross-correlation scoring
- **Onset detection**: `Sample::detect_onsets()` for REX-style slice points

### Effects
- **EffectChain**: Up to 5 serial slots routing through naad effects
- **Effect types**: Reverb, Delay, Chorus, Compressor, Limiter
- **Per-slot bypass** and wet/dry mix

### File I/O (`io` feature)
- **WAV loading**: `load_wav()`, `load_wav_from_memory()` via shravan
- **Streaming**: `StreamingWavReader` for chunked reading of large instruments
- Supports 8/16/24-bit integer and 32-bit float WAV

### Time-Stretching
- **WSOLA**: Waveform Similarity Overlap-Add with cross-correlation splice points
- **OLA**: Simple Overlap-Add for speech/mono
- **TimeStretcher**: Offline processing with configurable frame size and overlap

### Quality
- **114 unit tests + 4 doc-tests**
- **Serde roundtrip tests** for all public types
- **Send + Sync** assertions for all public types
- **`#[must_use]`** on all accessors, **`#[non_exhaustive]`** on all public enums
- **Fuzz targets** for SFZ and SF2 parsers (libfuzzer-sys)
- **no_std + alloc** support with `std` as default feature
