# nidhi — Roadmap to v1.0

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
- [ ] Replace one-pole lowpass with `naad` filter (true stereo)
- [ ] Per-zone ADSR config (wire SFZ `ampeg_*` opcodes through to zones)
- [ ] Per-zone filter envelope (`fileg_*` opcodes)
- [ ] Velocity curves via `naad` (linear, convex, concave, switch)

## v0.3.0

SFZ completeness and format support.

- [ ] SFZ `<control>` header support (default_path, etc.)
- [ ] SFZ `<curve>` header support
- [ ] SFZ `loop_sustain` mode (release exits loop)
- [ ] SF2/SoundFont import (basic region mapping)
- [ ] Crossfade loop points (configurable crossfade length)

## v0.4.0

Engine quality and time-stretching.

- [ ] Phase vocoder time-stretching (FFT-based, replace WSOLA fallback)
- [ ] Integrate TimeStretcher into engine (real-time grain mode)
- [ ] Voice group exclusion (e.g., hi-hat choke groups)
- [ ] Priority-based voice stealing (not just oldest)
- [ ] Legato and portamento modes
- [ ] Pitch bend support (per-engine, configurable range)

## v0.5.0

Modulation and performance.

- [ ] LFO modulation via `naad` (pitch, filter, amplitude)
- [ ] Modulation matrix (source -> destination routing)
- [ ] Per-voice pitch envelope
- [ ] Key tracking for filter cutoff
- [ ] Random/sequence round-robin modes
- [ ] SIMD-accelerated mixing and interpolation

## v1.0.0

Production-ready release.

- [ ] Full SFZ v1 opcode coverage
- [ ] Comprehensive benchmarks (voice count, buffer fill, interpolation)
- [ ] Public API audit (`#[must_use]`, `#[non_exhaustive]` on all public types)
- [ ] Documentation: all public items have doc comments with examples
- [ ] Serde roundtrip tests for all public types
- [ ] Fuzz testing for SFZ parser
- [ ] CHANGELOG.md with full history
- [ ] crates.io publish
