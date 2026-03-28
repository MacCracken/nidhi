# nidhi — Post-1.0 Roadmap

nidhi is the standalone sampler crate in the AGNOS ecosystem, replacing
`shruti-instruments::sampler`. Published to crates.io at 1.0.0.

## v1.1.0 — Performance + real-time safety

Zero-allocation render path, SIMD, and caching optimizations.

- [ ] Pre-allocated voice buffers (zero allocation in render path)
- [ ] Per-voice buffer accumulation (batch render, 5–10% perf)
- [ ] Filter coefficient caching (skip recompute when params unchanged)
- [ ] Envelope stage duration caching
- [ ] Denormal flushing in filter feedback paths (no_std path)
- [ ] Parameter smoothing via `naad::smoothing::ParamSmoother` (click-free param changes)
- [ ] SIMD-accelerated stereo mixing (accumulate L/R buffers)
- [ ] SIMD-accelerated cubic Hermite interpolation
- [ ] Benchmark suite: voice count scaling, buffer fill throughput, interpolation cost

## v1.2.0 — Advanced modulation + expression

Full modulation matrix and richer voice expression.

- [ ] Modulation matrix via `naad::mod_matrix::ModMatrix` (8x8 source→destination routing)
- [ ] Feed LFO/envelope/velocity/mod wheel/aftertouch/pitch bend as mod sources
- [ ] Per-voice pitch envelope (separate from amp ADSR)
- [ ] Amplitude LFO (tremolo) via zone config
- [ ] Random/sequence round-robin modes
- [ ] Portamento (glide between notes in mono/legato mode)

## v1.3.0 — Advanced time-stretching + grain

Real-time granular playback and FFT-based stretching.

- [ ] Integrate TimeStretcher into engine (real-time grain mode)
- [ ] Grain size configuration (10–100ms, configurable overlap)
- [ ] Phase vocoder time-stretching (FFT-based, replace WSOLA fallback)
- [ ] Pitch-independent time-stretch in render loop (0.25x–4.0x)
- [ ] Granular freeze mode (sustain a single grain position)

## Backlog

- [ ] Incorporate shravan 1.1.0 (not yet published)
- [ ] dhvani integration tested end-to-end (shruti consuming nidhi via dhvani)
- [ ] FLAC/OGG loading helpers in `io` module
