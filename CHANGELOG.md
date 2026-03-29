# Changelog

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
