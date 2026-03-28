# nidhi — Roadmap to v1.0

nidhi replaces `shruti-instruments::sampler` as the standalone sampler crate
in the AGNOS ecosystem. All sampling functionality migrates here; shruti
becomes a thin integration layer over nidhi via dhvani.

## v0.1.0 (current)

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

- [ ] Depend on `naad` for filters, envelopes, and modulation primitives
- [ ] Replace hand-rolled ADSR with `naad` envelope
- [ ] Replace one-pole lowpass with `naad` SVF filter (LP, HP, BP, Notch)
- [ ] True stereo filtering (independent filter state per channel)
- [ ] Per-zone ADSR config (wire SFZ `ampeg_*` opcodes through to zones)
- [ ] Per-zone filter envelope (`fileg_*` opcodes)
- [ ] Envelope + LFO modulation of filter cutoff (±4 octaves, from shruti)
- [ ] Velocity curves via `naad` (linear, convex, concave, switch)
- [ ] Smooth release from any envelope level

## v0.3.0

SFZ/SF2 completeness — feature parity with shruti's parsers.

- [ ] SFZ `<control>` header support (default_path, etc.)
- [ ] SFZ `<curve>` header support
- [ ] SFZ `loop_sustain` mode (release exits loop)
- [ ] SFZ note-name parsing (C1–G9, from shruti)
- [ ] Full SFZ v1 opcode coverage (~25+ opcodes, from shruti)
- [ ] SF2/SoundFont import — RIFF parser, preset/zone extraction, PCM to f32 (from shruti)
- [ ] Crossfade loop points (configurable crossfade length)

## v0.4.0

Voice management and expression — replace shruti voice.rs.

- [ ] VoiceStealMode enum: Oldest, Quietest, Lowest, None (from shruti)
- [ ] Voice group exclusion / choke groups (e.g., hi-hat)
- [ ] Per-note pitch bend (from shruti)
- [ ] Per-note pressure / aftertouch (from shruti)
- [ ] Per-note brightness CC#74 (from shruti)
- [ ] Legato and portamento modes
- [ ] Pitch bend support (per-engine, configurable range)

## v0.5.0

Slicing, time-stretching, and grain playback.

- [ ] REX-style slice points with auto-onset detection (from shruti)
- [ ] Phase vocoder time-stretching (FFT-based, replace WSOLA fallback)
- [ ] Integrate TimeStretcher into engine (real-time grain mode, 0.25x–4.0x)
- [ ] Grain size configuration (10–100ms, from shruti)

## v0.6.0

Modulation and routing — replace shruti mod_matrix.rs + lfo.rs.

- [ ] LFO modulation via `naad` (pitch, filter, amplitude)
- [ ] LFO waveforms: sine, triangle, square, sawtooth (from shruti)
- [ ] LFO sync to transport
- [ ] Modulation matrix — 8x8 source/destination routing (from shruti)
- [ ] Per-voice pitch envelope
- [ ] Key tracking for filter cutoff
- [ ] Random/sequence round-robin modes

## v0.7.0

Effects, presets, and integration.

- [ ] Per-instrument effect chain (5 slots, from shruti)
- [ ] Effect parameter automation
- [ ] Effect bypass and routing
- [ ] Preset serialization/deserialization (JSON via serde, from shruti)
- [ ] Drum machine mode — 16-pad engine with step sequencer (from shruti)
- [ ] Pattern banks (A/B/C/D x 16 presets, from shruti)
- [ ] Velocity layers per pad (from shruti)
- [ ] Per-pad effect sends (from shruti)

## v0.8.0

Performance and real-time safety.

- [ ] SIMD-accelerated mixing and interpolation
- [ ] Pre-allocated voice buffers (zero allocation in render path)
- [ ] Envelope stage duration caching (from shruti backlog)
- [ ] Filter coefficient caching (from shruti backlog)
- [ ] Denormal flushing in filter feedback (from shruti backlog)
- [ ] Per-voice buffer accumulation (5–10% perf, from shruti backlog)

## v1.0.0

Production-ready release. Full replacement for shruti-instruments sampler.

- [ ] Feature parity with `shruti-instruments::sampler` (verified)
- [ ] dhvani integration tested (shruti consuming nidhi via dhvani)
- [ ] Full SFZ v1 opcode coverage
- [ ] Comprehensive benchmarks (voice count, buffer fill, interpolation)
- [ ] Public API audit (`#[must_use]`, `#[non_exhaustive]` on all public types)
- [ ] Documentation: all public items have doc comments with examples
- [ ] Serde roundtrip tests for all public types
- [ ] Fuzz testing for SFZ and SF2 parsers
- [ ] CHANGELOG.md with full history
- [ ] crates.io publish
