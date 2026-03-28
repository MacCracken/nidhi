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

- [x] Replace hand-rolled voice allocation with `naad::VoiceManager` (std-gated)
- [x] Expose `StealMode` (Oldest, Quietest, Lowest, None) and `PolyMode` (Poly, Mono, Legato)
- [x] Wire per-note pitch bend, pressure, brightness
- [x] Voice group exclusion / choke groups (e.g., hi-hat)
- [x] Pitch bend range config (per-engine, default ±2 semitones)
- [x] Apply pitch bend + pressure + brightness in render loop

## v0.5.0

Slicing, time-stretching, and grain playback.

- [x] REX-style slice points with auto-onset detection on Sample
- [x] Per-zone time-stretch ratio config
- [ ] Phase vocoder time-stretching (FFT-based — deferred, needs FFT dependency)
- [ ] Real-time grain mode in engine (deferred to v0.8.0)

## v0.6.0

Adopt `naad` modulation + routing.

- [x] Pitch LFO via `naad::modulation::Lfo` (per-voice, from zone config)
- [x] Filter LFO via `naad::modulation::Lfo` (per-voice, from zone config)
- [x] Key tracking for filter cutoff (distance from C4)
- [x] SFZ opcodes: pitchlfo_freq, pitchlfo_depth, fillfo_freq, fillfo_depth, fil_keytrack
- [ ] Modulation matrix exposure on engine (deferred — naad ModMatrix available, wire when needed)
- [ ] Parameter smoothing via `naad::smoothing::ParamSmoother` (deferred to v0.8.0)

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
