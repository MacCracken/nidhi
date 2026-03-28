# nidhi — Roadmap to v1.0

nidhi replaces `shruti-instruments::sampler` as the standalone sampler crate
in the AGNOS ecosystem. All sampling functionality migrates here; shruti
becomes a thin integration layer over nidhi via dhvani.

## v0.1.0

Foundation release. Core sampler engine with polyphonic playback.

- **Engine**: Polyphonic voice management with oldest-voice stealing
- **Zones**: Key/velocity mapping, round-robin groups, root note + tune
- **Envelopes**: Per-voice ADSR with sample-accurate linear stages
- **Loop modes**: OneShot, Forward, PingPong, Reverse
- **Interpolation**: Cubic Hermite for pitch-shifted playback
- **Filtering**: One-pole lowpass per voice with velocity tracking
- **Pan**: Per-zone constant-power stereo panning
- **SFZ import**: Parser with global/group/region inheritance
- **Time-stretching**: WSOLA and OLA algorithms (offline)
- **Sample bank**: Indexed storage with mono and stereo support
- **no_std**: Full no_std + alloc support

## v0.2.0

Adopt naad primitives and per-zone envelopes.

- [x] Depend on `naad` for filters, envelopes, and modulation primitives
- [x] Replace hand-rolled ADSR with `naad` envelope (`AmpEnvelope` wrapper)
- [x] Replace one-pole lowpass with `naad` SVF filter (LP, HP, BP, Notch)
- [x] True stereo filtering (independent filter state per channel)
- [x] Per-zone ADSR config (wire SFZ `ampeg_*` opcodes through to zones)
- [x] Per-zone filter envelope (`fileg_*` opcodes)
- [x] Velocity curves (Linear, Convex, Concave, Switch)
- [x] Smooth release from any envelope level

## v0.3.0

SFZ/SF2 completeness — feature parity with shruti's parsers.

- [x] SFZ `<control>` header support (default_path)
- [x] SFZ `<curve>` header support (stub, parsed without breaking)
- [x] SFZ `loop_sustain` mode (LoopSustain — release exits loop)
- [x] SFZ note-name parsing (C-1 through G9, sharps/flats)
- [x] SFZ `key` shorthand, `transpose`, `offset`, `end`, `fil_type`, `resonance`
- [x] SF2/SoundFont import — RIFF parser, preset/zone extraction, PCM to f32
- [x] Crossfade loop points (configurable crossfade length)
- [x] Sample offset/end fields on Zone

## v0.4.0

Adopt `naad::voice::VoiceManager` + choke groups + expression.

- [ ] Replace hand-rolled voice allocation with `naad::VoiceManager` (std-gated)
- [ ] Expose `StealMode` (Oldest, Quietest, Lowest, None) and `PolyMode` (Poly, Mono, Legato)
- [ ] Wire per-note pitch bend, pressure, brightness from `naad::voice::Voice`
- [ ] Voice group exclusion / choke groups (e.g., hi-hat)
- [ ] Pitch bend range config (per-engine, default ±2 semitones)
- [ ] Apply pitch bend + pressure + brightness in render loop

## v0.5.0

Slicing, time-stretching, and grain playback.

- [ ] REX-style slice points with auto-onset detection (from shruti)
- [ ] Phase vocoder time-stretching (FFT-based, replace WSOLA fallback)
- [ ] Integrate TimeStretcher into engine (real-time grain mode, 0.25x–4.0x)
- [ ] Grain size configuration (10–100ms, from shruti)

## v0.6.0

Adopt `naad` modulation + routing.

- [ ] LFO modulation via `naad::modulation::Lfo` (pitch, filter, amplitude)
- [ ] Modulation matrix via `naad::mod_matrix::ModMatrix` (8x8 routing)
- [ ] Per-voice pitch envelope
- [ ] Key tracking for filter cutoff
- [ ] Parameter smoothing via `naad::smoothing::ParamSmoother`

## v0.7.0

Effects, presets, and integration.

- [ ] Per-instrument effect chain (5 slots, from shruti)
- [ ] Preset serialization/deserialization (JSON via serde, from shruti)
- [ ] Drum machine mode — 16-pad engine with step sequencer (from shruti)
- [ ] Pattern banks + velocity layers per pad (from shruti)

## v0.8.0

Performance and real-time safety.

- [ ] SIMD-accelerated mixing and interpolation
- [ ] Pre-allocated voice buffers (zero allocation in render path)
- [ ] Filter coefficient caching
- [ ] Denormal flushing in filter feedback
- [ ] Per-voice buffer accumulation (5–10% perf)

## v1.0.0

Production-ready release. Full replacement for shruti-instruments sampler.

- [ ] Feature parity with `shruti-instruments::sampler` (verified)
- [ ] dhvani integration tested (shruti consuming nidhi via dhvani)
- [ ] Comprehensive benchmarks (voice count, buffer fill, interpolation)
- [ ] Public API audit (`#[must_use]`, `#[non_exhaustive]` on all public types)
- [ ] Documentation: all public items have doc comments with examples
- [ ] Serde roundtrip tests for all public types
- [ ] Fuzz testing for SFZ and SF2 parsers
- [ ] CHANGELOG.md with full history
- [ ] crates.io publish
