# nidhi — Post-1.0 Roadmap

nidhi is the standalone sampler crate in the AGNOS ecosystem, replacing
`shruti-instruments::sampler`. Published to crates.io at 1.0.0.

## v1.1.0 — Performance + real-time safety ✓

Zero-allocation render path, SIMD, and caching optimizations. Released 2026-03-28.

- [x] Benchmark suite: voice count scaling, buffer fill throughput, interpolation cost
- [x] Pre-allocated voice buffers (zero allocation in render path)
- [x] Fix per-sample Vec allocation in `fill_buses_stereo`
- [x] Per-voice buffer accumulation (batch render into temp buffer, then SIMD mix-down)
- [x] Filter coefficient caching (epsilon check on cutoff, skip recompute when unchanged)
- [x] Envelope stage duration caching (no-op — already cheap, naad handles internally)
- [x] Denormal flushing via `flush_denormal()` in no_std filter feedback + envelope release
- [x] Parameter smoothing via `naad::smoothing::ParamSmoother` (filter cutoff modulation)
- [x] SIMD-accelerated stereo mixing (`core::arch` SSE2/NEON, behind `simd` feature)
- [x] SIMD-accelerated cubic Hermite interpolation (L/R in f32x4, behind `simd` feature)

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

- [x] ~~Incorporate shravan~~ — done in 1.0.1 (replaced hound with shravan 1.0.1)
- [ ] dhvani integration tested end-to-end (shruti consuming nidhi via dhvani)
- [ ] FLAC/AIFF/OGG loading helpers in `io` module (shravan now provides codecs)
