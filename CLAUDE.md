# nidhi — Claude Code Instructions

## Project Identity

**nidhi** (Sanskrit: treasure) — Sample playback engine for AGNOS

- **Type**: Flat library crate
- **License**: GPL-3.0
- **MSRV**: 1.89
- **Version**: SemVer (see VERSION file)

## Consumers

dhvani (audio engine), shruti (DAW — nidhi replaces `shruti-instruments::sampler` via dhvani), and any AGNOS component needing sample playback with key/velocity zones, loop modes, or time-stretching.

## Dependencies

- **naad**: Audio synthesis primitives — filters, envelopes, modulation, voice management, effects (optional, behind `std`)
- **shravan**: Audio codecs — WAV, FLAC, AIFF, streaming (optional, behind `io`)
- **hisab**: Math primitives (num features)
- **serde**: Serialization (derive, alloc)
- **thiserror**: Error types
- **tracing**: Instrumentation
- **tracing-subscriber**: Optional, behind `logging` feature

## Work Loop

1. Read the relevant code before proposing changes
2. Make the change
3. `cargo fmt`
4. `cargo clippy --all-features --all-targets -- -D warnings`
5. `cargo test --all-features`
6. `cargo test --doc`
7. `cargo check --no-default-features` (no_std verification)
8. `cargo bench` (if performance-relevant)

## Task Sizing

- **Small**: Single-function change, test fix, doc tweak
- **Medium**: New playback mode, new import format, test suite expansion
- **Large**: New module, engine architecture change, format migration

## Key Principles

- Never skip benchmarks
- `#[non_exhaustive]` on ALL public enums
- `#[must_use]` on all pure functions and accessors
- Every type must be Serialize + Deserialize (serde)
- Zero unwrap/panic in library code (`.expect()` only on provably infallible paths)
- All types must have serde roundtrip tests
- Playback accuracy over speed
- Sample-accurate loop points and crossfades

## Module Structure

- `capture.rs` — Sample recording, trim, normalize, loop detection
- `effect_chain.rs` — Per-instrument effect chain (naad effects)
- `engine.rs` — Core playback engine, voice management, render loop
- `envelope.rs` — AmpEnvelope (naad wrapper / no_std fallback), AdsrConfig
- `error.rs` — NidhiError, Result type alias
- `instrument.rs` — Instrument (zone collection, round-robin)
- `io.rs` — WAV file loading and streaming (behind `io` feature)
- `lib.rs` — Crate root, feature gates, prelude, trait assertions, serde tests
- `loop_mode.rs` — LoopMode enum (OneShot, Forward, PingPong, Reverse, LoopSustain)
- `sample.rs` — Sample data, SampleBank, onset detection, slice points
- `sf2.rs` — SF2/SoundFont binary parser
- `sfz.rs` — SFZ text parser (v1 + v2 subset)
- `stretch.rs` — Time-stretching (WSOLA, OLA)
- `zone.rs` — Zone config (key/vel, filter, envelope, LFO, bus routing)

## DO NOT

- **Do not commit or push** — the user handles all git operations
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies
- Do not skip benchmarks before claiming performance improvements
