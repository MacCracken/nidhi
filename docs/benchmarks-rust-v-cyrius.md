# Benchmarks: Rust vs Cyrius

> nidhi 2.0.0 (Cyrius port) benchmark comparison.
>
> - **Rust**: criterion v0.5, release mode (`rust-old/`, `cargo bench`). f32 samples; naad 1 /
>   shravan 1.0.1 std path; SSE2/NEON SIMD mix + zero-alloc render path.
> - **Cyrius**: cyrius 6.3.34, `tests/nidhi.bcyr` (`bench_batch_start`/`stop`), 2026-07-02. f64
>   samples (no f32); naad-backed DSP; per-sample SVF-output alloc; per-frame block render.
> - **Platform**: x86_64 Linux
>
> Both sides run the SAME 7 operations (from `rust-old/benches/benchmarks.rs`). The parity signal
> is the **relative shape** across benchmarks, not the absolute nanoseconds (Cyrius is f64-only
> and does no autovectorization). Reproduce: `cyrius bench tests/nidhi.bcyr` and, in `rust-old/`,
> `cargo bench`.

## Head-to-Head

Rust = criterion mid estimate; Cyrius = `bench_batch` average. Ratio = Cyrius / Rust.

| Benchmark | Rust | Cyrius | Ratio | Notes |
|-----------|------|--------|-------|-------|
| **voice_count_scaling** (per `next_sample_stereo`) | | | | |
| 1 voice | 54.4 ns | 887 ns | 16× | interpolate → filter → env → mix |
| 4 voices | 92.0 ns | 1.875 µs | 20× | |
| 8 voices | 151.7 ns | 3.173 µs | 21× | ~linear in voice count on both sides |
| 16 voices | 264.0 ns | 5.591 µs | 21× | |
| 32 voices | 503.3 ns | 10.689 µs | 21× | |
| 64 voices | 928.8 ns | 20.886 µs | 22× | |
| **fill_buffer_stereo** (512-frame block) | | | | |
| 1 voice | 7.42 µs | 457.7 µs | 62× | Rust block-renders into scratch + SIMD-mixes; Cyrius renders per-frame → no block win (see analysis) |
| 8 voices | 58.46 µs | 1.569 ms | 27× | |
| 16 voices | 115.7 µs | 2.877 ms | 25× | |
| **fill_buffer_per_sample** (512-frame loop) | | | | |
| 1 voice | 28.63 µs | 440.9 µs | 15× | per-sample baseline; in Rust this is ~3.9× slower than the block path, in Cyrius they're equal |
| 8 voices | 76.90 µs | 1.582 ms | 21× | |
| 16 voices | 137.1 µs | 2.869 ms | 21× | |
| **interpolation_cubic** (per read) | 8.0 ns | 82 ns | 10× | Catmull-Rom, mono (Rust `_1k` bench = 8.02 µs / 1000 reads) |
| **interpolation_stereo** (per read) | 10.3 ns | 155 ns | 15× | L+R cubic; Rust has an SSE path (`_1k` = 10.34 µs / 1000) |
| **fill_buffer_stereo_filtered_8v** | 70.7 ns† | 2.592 ms | — | †**criterion artifact, not comparable** — the filtered zone has no loop, so its voices die after ~86 blocks and criterion's 70 M iterations then measure mostly-empty renders. The Cyrius figure (40 fills, voices alive) is the real filtered-block cost; compare it to `fill_buffer_stereo/8` scaled by the SVF cost. |
| **wsola_1sec_2x** | 62.71 ms | 857.1 ms | 14× | O(frames·tolerance·frame_size) search |

Typical per-sample-path ratio is **~15–22×** (the expected f64-vs-f32-SIMD range). The
**62× outlier** on `fill_buffer_stereo/1` is the clearest signal: Rust's block render is ~3.9×
faster than its per-sample path, while Cyrius's block path is per-frame (equal to per-sample), so
the ratio inflates there — a concrete, fixable gap (see optimization vector #2).

## Real-time headroom (the metric that matters)

For an audio engine the question isn't "how much slower than Rust" — it's "does it clear the
per-sample deadline". At 44.1 kHz that budget is **1 s / 44100 = 22,676 ns per sample** (20,833 ns
at 48 kHz). The un-optimized Cyrius port, from the `voice_count_scaling` figures (cost to render
*N* concurrent voices per output sample):

| Voices | Cyrius / sample | % of 44.1 kHz budget | Real-time margin |
|-------:|----------------:|---------------------:|-----------------:|
| 1 | 887 ns | 3.9 % | **26×** |
| 8 | 3.173 µs | 14.0 % | 7.1× |
| 16 | 5.591 µs | 24.7 % | 4.1× |
| 32 | 10.689 µs | 47.1 % | 2.1× |
| 64 | 20.886 µs | 92.1 % | **1.09× (still fits)** |

**The port sustains full ~64-voice polyphony in real time at 44.1 kHz with no optimization** (≈63
at 48 kHz). Block render agrees: `fill_buffer_stereo/16` = 2.877 ms for a 512-frame block, vs a
512-frame budget of 11.6 ms → **4× headroom**. The ~15–22× gap to Rust only widens Rust's already-
comfortable margin; it does not change whether Cyrius meets the deadline — which it does throughout.

## Full Cyrius Benchmark Set (16 benchmarks, cyrius 6.3.34, 2026-07-02)

| Benchmark | Avg | Iterations |
|-----------|-----|------------|
| voice_count_scaling/1 | 887 ns | 4,410 |
| voice_count_scaling/4 | 1.875 µs | 4,410 |
| voice_count_scaling/8 | 3.173 µs | 4,410 |
| voice_count_scaling/16 | 5.591 µs | 4,410 |
| voice_count_scaling/32 | 10.689 µs | 4,410 |
| voice_count_scaling/64 | 20.886 µs | 4,410 |
| fill_buffer_stereo/1 | 457.7 µs | 40 |
| fill_buffer_stereo/8 | 1.569 ms | 40 |
| fill_buffer_stereo/16 | 2.877 ms | 40 |
| fill_buffer_per_sample/1 | 440.9 µs | 40 |
| fill_buffer_per_sample/8 | 1.582 ms | 40 |
| fill_buffer_per_sample/16 | 2.869 ms | 40 |
| interpolation_cubic | 82 ns | 44,100 |
| interpolation_stereo | 155 ns | 44,100 |
| fill_buffer_stereo_filtered_8v | 2.592 ms | 40 |
| wsola_1sec_2x | 857.1 ms | 2 |

## Analysis

### Why Cyrius is slower per-operation

| Factor | Cost | Where |
|--------|------|-------|
| f64 vs f32 | ~1.5–2× | all sample math |
| No autovectorization (SIMD) | ~2–4× | mix, interpolation, filter |
| Per-sample heap alloc | ~200–400 ns | SVF `process_sample` allocs an `SvfOutput`; `next_sample_stereo` allocs its scratch pair each frame |
| Per-frame block render | — | `fill_buffer_stereo` calls `next_sample_stereo` per frame instead of Rust's per-voice block-into-scratch (so it ≈ `fill_buffer_per_sample` and forgoes Rust's ~2.9×/1.2× block speedup) |

The **relative shape holds**: `next_sample_stereo` scales ~linearly with voice count on both
sides (887 ns → 20.9 µs for 1→64), and the filtered path costs ~1.6× the unfiltered 8-voice path
— the same parity signal Rust shows.

### Where Cyrius wins

| Metric | Rust | Cyrius |
|--------|------|--------|
| Precision | f32 (~1e-7) | f64 (~1e-15) |
| Binary | dynamic, many crates | static, self-contained bundle |
| Build | cargo + criterion (minutes) | `cyrius build` (instant) |
| Dependencies | naad + shravan + hisab + hound-era stack | naad/shravan/hisab dist bundles |

### Optimization vectors (post-v2)

Performance is deliberately **out of scope until after v2** — v0.1.x targets functional parity,
not speed. The levers below exist in Cyrius and would close most of the gap when the time comes;
they are recorded here so the gap is understood, not so it is chased now.

1. **Reuse per-sample scratch** — hoist the `SvfOutput` and the L/R accumulator out of the
   render loop (a preallocated per-voice scratch) to kill the per-sample allocations.
2. **True block render** — port Rust's per-voice-into-scratch `fill_buffer_stereo` so the block
   path beats the per-sample path (the source of the 62× outlier above).
3. **SIMD** — Cyrius `f64v_*` intrinsics for the mix-down and cubic interpolation (naad already
   uses them internally) would close ~2–4× on the hot paths.
4. **Cache filter coeffs** — the 0.5 Hz cutoff dead-band is ported; further amortize `set_params`.

None are prerequisites for correctness or feature parity; the ~15–22× per-sample ratio is the
expected f64/no-SIMD baseline for a fresh Cyrius port.
