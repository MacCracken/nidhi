# nidhi — Post-1.0 Roadmap

nidhi is the standalone sampler crate in the AGNOS ecosystem, replacing
`shruti-instruments::sampler`. Published to crates.io at 1.0.0.

## v1.1.0 — Performance

- [ ] SIMD-accelerated mixing and interpolation
- [ ] Pre-allocated voice buffers (zero allocation in render path)
- [ ] Filter coefficient caching
- [ ] Denormal flushing in filter feedback
- [ ] Per-voice buffer accumulation (5–10% perf)
- [ ] Parameter smoothing via `naad::smoothing::ParamSmoother`

## v1.2.0 — Advanced modulation

- [ ] Modulation matrix exposure on engine (naad `ModMatrix`)
- [ ] Per-voice pitch envelope
- [ ] Random/sequence round-robin modes

## v1.3.0 — Advanced time-stretching

- [ ] Real-time grain mode in engine (TimeStretcher integration, 0.25x–4.0x)
- [ ] Phase vocoder time-stretching (FFT-based, replace WSOLA fallback)
- [ ] Grain size configuration (10–100ms)

## Backlog

- [ ] Comprehensive benchmarks (voice count, buffer fill, interpolation)
- [ ] Fuzz testing for SFZ and SF2 parsers
- [ ] dhvani integration tested (shruti consuming nidhi via dhvani)
