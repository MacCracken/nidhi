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

## Development Process

### P(-1): Scaffold Hardening (before any new features)

0. Read roadmap, CHANGELOG, and open issues
1. Test + benchmark sweep of existing code
2. Cleanliness check: `cargo fmt --check`, `cargo clippy --all-features --all-targets -- -D warnings`, `cargo audit`, `cargo deny check`, `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`
3. Get baseline benchmarks
4. Internal deep review
5. External research -- sample playback specs, SFZ/SF2 formats, DSP algorithms
6. Cleanliness check -- must be clean after review
7. Additional tests/benchmarks from findings
8. Post-review benchmarks
9. Repeat if heavy

### Work Loop (continuous)

1. Work phase
2. Cleanliness check
3. Test + benchmark additions
4. Run benchmarks
5. Internal review
6. Cleanliness check
7. Deeper tests/benchmarks
8. Benchmarks again
9. If review heavy -> return to step 5
10. Documentation -- CHANGELOG, roadmap, docs
11. Version check
12. Return to step 1

### Cleanliness Check

```bash
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo test --doc
cargo check --no-default-features
cargo audit
cargo deny check
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
cargo bench
```

## Task Sizing

- **Small**: Single-function change, test fix, doc tweak
- **Medium**: New playback mode, new import format, test suite expansion
- **Large**: New module, engine architecture change, format migration

## Key Principles

- Never skip benchmarks
- `#[non_exhaustive]` on ALL public enums
- `#[must_use]` on all pure functions and accessors
- `#[inline]` on hot-path render and sample processing functions
- Every type must be Serialize + Deserialize (serde)
- Feature-gate optional modules
- Zero unwrap/panic in library code (`.expect()` only on provably infallible paths)
- All types must have serde roundtrip tests
- `no_std` compatible (with alloc)
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
- Do not break backward compatibility without a major version bump
- Do not skip benchmarks before claiming performance improvements
