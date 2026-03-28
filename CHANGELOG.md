# Changelog

## 1.0.0 — 2026-03-28

Stable release. Full-featured sample playback engine for AGNOS.

- **Sample capture**: `SampleRecorder` for recording audio input into samples, `trim_silence()`, `normalize_peak()`, `normalize_rms()`, `detect_loop_points()` (zero-crossing + cross-correlation)
- **Effect chain**: Per-instrument `EffectChain` with up to 5 serial slots routing through naad effects (Reverb, Delay, Chorus, Compressor, Limiter), per-slot bypass and wet/dry mix
- **Preset serialization**: All public types derive `Serialize` + `Deserialize` for full engine state persistence
- **Serde roundtrip tests**: Comprehensive coverage for all public types
- **Public API audit**: `#[must_use]` on all accessors, `#[non_exhaustive]` on all public enums, `Send + Sync` assertions for all public types
- **VERSION file** and `bump-version.sh` script

## 0.7.0 — 2026-03-28

Sample capture, effect chain, and preset serialization. See 1.0.0 above (released together).

## 0.6.0 — 2026-03-28

Per-voice modulation via naad LFOs and filter key tracking.

- **Pitch LFO**: Per-voice pitch modulation via `naad::modulation::Lfo` (sine, from zone config)
- **Filter LFO**: Per-voice filter cutoff modulation via `naad::modulation::Lfo`
- **Key tracking**: Filter cutoff scales with note distance from C4 (0.0–1.0 range)
- **SFZ opcodes**: `pitchlfo_freq`, `pitchlfo_depth`, `fillfo_freq`, `fillfo_depth`, `fil_keytrack`
- All modulation wired into render loop (pitch LFO → speed, filter LFO + keytrack → cutoff)

## 0.5.0 — 2026-03-28

Slicing and time-stretch configuration.

- **Onset detection**: `Sample::detect_onsets()` — energy-based transient detection with configurable threshold and minimum slice distance
- **Slice points**: `Sample::with_slices()` / `slices()` for REX-style slice management
- **Time-stretch ratio**: Per-zone `with_time_stretch()` config (0.25x–4.0x)

## 0.4.0 — 2026-03-28

Adopted `naad::voice::VoiceManager` for voice allocation and added expression support.

- **Voice management**: `naad::VoiceManager` (std-gated) replaces hand-rolled allocation
- **Steal modes**: `StealMode` enum — Oldest, Quietest, Lowest, None
- **Poly modes**: `PolyMode` enum — Poly, Mono, Legato
- **Per-note expression**: `apply_pitch_bend()`, `apply_pressure()`, `apply_brightness()`
- **Pitch bend**: Applied in render loop via speed ratio modulation, configurable range (default ±2 semitones)
- **Brightness**: CC#74 modulates filter cutoff (0.0 = dark, 0.5 = neutral, 1.0 = bright)
- **Pressure**: Aftertouch modulates amplitude (±20%)
- **Choke groups**: `Zone::with_choke_group()` — voices in same group silence each other on note-on

## 0.3.0 — 2026-03-28

SFZ/SF2 format completeness.

- **SF2 parser**: New `sf2` module — RIFF binary parsing, preset/instrument/zone extraction, PCM16→f32 conversion, returns nidhi-native types (`Instrument`, `SampleBank`, `Sf2Preset`)
- **LoopSustain**: New loop mode — loops while held, plays through on release. SF2 mode 3 maps here.
- **SFZ `<control>`**: `default_path` opcode prepends path to sample filenames
- **SFZ `<curve>`**: Parsed without breaking (stub for future use)
- **Note-name parsing**: `parse_note_or_number()` supports C-1 through G9 with sharps/flats
- **SFZ opcodes**: `key` (shorthand), `transpose`, `offset`, `end`, `fil_type`, `resonance`/`fil_resonance`
- **Crossfade loops**: `Zone::with_crossfade()` — linear blend at loop boundaries
- **Sample bounds**: `Zone::with_sample_offset()` / `with_sample_end()` for partial playback

## 0.2.0 — 2026-03-28

Adopted naad primitives for filters, envelopes, and velocity curves.

- **naad dependency**: Optional, gated behind `std` feature (default). Hand-rolled fallbacks for no_std.
- **AmpEnvelope**: New per-voice envelope wrapper — `naad::envelope::Adsr` (std) or built-in linear ADSR (no_std). Smooth release from any level.
- **SVF filter**: Replaced one-pole lowpass with `naad::filter::StateVariableFilter` — true stereo (independent L/R state), LP/HP/BP/Notch modes via `FilterMode` enum
- **Per-zone ADSR**: `Zone::with_adsr()` overrides engine default. SFZ `ampeg_*` opcodes now wired through (previously parsed but discarded).
- **Filter envelope**: `Zone::with_filter_envelope()` + `fileg_*` SFZ opcodes. Modulates cutoff per-sample in render loop.
- **Velocity curves**: `VelocityCurve` enum — Linear, Convex, Concave, Switch. Applied per-zone in `note_on`.

## 0.1.0 — 2026-03-27

Initial release. Core sampler engine with polyphonic playback.

- **SamplerEngine**: Polyphonic voice pool with oldest-voice stealing
- **Zone**: Key/velocity mapping, root note, fine tuning, volume, pan, round-robin groups
- **ADSR**: Per-voice amplitude envelope with sample-accurate linear stages
- **Loop modes**: OneShot, Forward, PingPong, Reverse
- **Interpolation**: Cubic Hermite for pitch-shifted playback
- **Filtering**: One-pole lowpass per voice with velocity tracking
- **SFZ import**: Parser with `<global>`/`<group>`/`<region>` inheritance, 15+ opcodes
- **Time-stretching**: WSOLA and OLA algorithms (offline `TimeStretcher`)
- **Sample bank**: Indexed storage with mono/stereo support, cubic interpolation
- **no_std**: Full `no_std` + `alloc` support with `std` as default feature
