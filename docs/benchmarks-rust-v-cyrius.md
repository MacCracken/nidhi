# Benchmarks: Rust vs Cyrius

> nidhi 2.1.0 (Cyrius port) benchmark comparison.
>
> - **Rust**: criterion v0.5, release mode (`cargo bench` in the retired `rust-old/`). f32 samples; naad 1 /
>   shravan 1.0.1 std path; SSE2/NEON SIMD mix + zero-alloc render path.
> - **Cyrius**: cyrius 6.5.36, `tests/nidhi.bcyr` (`bench_batch_start`/`stop`), 2026-08-31. f64
>   samples (no f32); naad 2.2.2 / shravan 2.8.0 / hisab 2.11.2; per-frame block render.
>   ⚠ **The ratios below moved substantially in 2.0.7**, which took WSOLA 4.02x, interpolation
>   2.8x and 64-voice scaling 1.64x — all bit-identical. Earlier revisions of this document
>   quoted 2.0.1 figures; do not compare against those.
> - **Platform**: x86_64 Linux
>
> Both sides run the SAME 7 operations (from `rust-old/benches/benchmarks.rs`). The parity signal
> is the **relative shape** across benchmarks, not the absolute nanoseconds (Cyrius is f64-only
> and does no autovectorization). Reproduce the Cyrius side with `cyrius bench tests/nidhi.bcyr`.
> The Rust side is **not reproducible from the working tree** — `rust-old/` was retired in 2.0.4.
> Its numbers survive in `bench-history-rust.csv` (tracked since 2.0.4); to re-measure, restore
> the crate from git history and see `docs/port/oracle-build-identity.md` for the exact
> dependency versions and rustc.

## Head-to-Head

Rust = criterion mid estimate; Cyrius = `bench_batch` average. Ratio = Cyrius / Rust.

| Benchmark | Rust | Cyrius | Ratio | Notes |
|-----------|------|--------|-------|-------|
| **voice_count_scaling** (per `next_sample_stereo`) | | | | |
| 1 voice | 54.4 ns | 720 ns | 13× | interpolate → filter → env → mix |
| 4 voices | 92.0 ns | 1.293 µs | 14× | |
| 8 voices | 151.7 ns | 2.120 µs | 14× | ~linear in voice count on both sides |
| 16 voices | 264.0 ns | 3.644 µs | 14× | |
| 32 voices | 503.3 ns | 6.970 µs | 14× | |
| 64 voices | 928.8 ns | 13.245 µs | 14× | |
| **fill_buffer_stereo** (512-frame block) | | | | |
| 1 voice | 7.42 µs | 364.3 µs | 49× | Rust block-renders into scratch + SIMD-mixes; Cyrius renders per-frame → no block win (see analysis) |
| 8 voices | 58.46 µs | 1.113 ms | 19× | |
| 16 voices | 115.7 µs | 1.901 ms | 16× | |
| **fill_buffer_per_sample** (512-frame loop) | | | | |
| 1 voice | 28.63 µs | 361.4 µs | 13× | per-sample baseline; in Rust this is ~3.9× slower than the block path, in Cyrius they're equal |
| 8 voices | 76.90 µs | 1.080 ms | 14× | |
| 16 voices | 137.1 µs | 1.904 ms | 14× | |
| **interpolation_cubic** (per read) | 8.0 ns | 37 ns | 4.6× | Catmull-Rom, mono (Rust `_1k` bench = 8.02 µs / 1000 reads) |
| **interpolation_stereo** (per read) | 10.3 ns | 60 ns | 5.8× | L+R cubic; Rust has an SSE path (`_1k` = 10.34 µs / 1000) |
| **fill_buffer_stereo_filtered_8v** | 70.7 ns† | 1.511 ms | — | †**criterion artifact, not comparable** — the filtered zone has no loop, so its voices die after ~86 blocks and criterion's 70 M iterations then measure mostly-empty renders. The Cyrius figure (40 fills, voices alive) is the real filtered-block cost; compare it to `fill_buffer_stereo/8` scaled by the SVF cost. |
| **wsola_1sec_2x** | 62.71 ms | 217.7 ms | 3.5× | O(frames·tolerance·frame_size) search |

Typical per-sample-path ratio is now **~13–16×**, down from ~15–22× before 2.0.7 — and
interpolation, which was 10–15×, is **4.6–5.8×**. That is the closest the port gets to Rust
anywhere, and it is where the f64-vs-f32-SIMD gap should be narrowest: the work is loads and
multiply-adds with no transcendentals.

The **49× outlier** on `fill_buffer_stereo/1` is still the clearest remaining signal (it was
62×). Rust's block render is ~3.9× faster than its per-sample path; Cyrius's block path is
per-frame, so the two are equal and the ratio inflates there. That gap is the voice-major block
render, still open — see the roadmap, where it is deferred because the obvious version measured
*worse* at ≥8 voices.

`wsola_1sec_2x` fell from 14× to **3.5×**, the largest single move: the correlation search stopped
paying `vec_get`'s bounds checks on ~88M element pairs.

## Real-time headroom (the metric that matters)

For an audio engine the question isn't "how much slower than Rust" — it's "does it clear the
per-sample deadline". At 44.1 kHz that budget is **1 s / 44100 = 22,676 ns per sample** (20,833 ns
at 48 kHz). From the `voice_count_scaling` figures (cost to render *N* concurrent voices per
output sample), at nidhi 2.1.0:

| Voices | Cyrius / sample | % of 44.1 kHz budget | Real-time margin | (was, 2.0.1) |
|-------:|----------------:|---------------------:|-----------------:|-------------:|
| 1 | 720 ns | 3.2 % | **31×** | 26× |
| 8 | 2.120 µs | 9.3 % | 10.7× | 7.3× |
| 16 | 3.644 µs | 16.1 % | 6.2× | 4.0× |
| 32 | 6.970 µs | 30.7 % | 3.3× | 2.1× |
| 64 | 13.245 µs | 58.4 % | **1.7×** | 1.09× |

**64-voice polyphony now clears the deadline with 1.7× margin**, where before 2.0.7 it fit at
1.09× — comfortably enough that the headroom is no longer the interesting number. Block render
agrees: `fill_buffer_stereo/16` = 1.901 ms against a 512-frame budget of 11.6 ms → **6.1×
headroom**, up from 4×.

The remaining ~13–16× gap to Rust widens Rust's already-comfortable margin; it does not change
whether Cyrius meets the deadline, which it does throughout and by a wider margin than at any
point in the 2.0.x line.

## Full Cyrius Benchmark Set (17 benchmarks, cyrius 6.5.36, nidhi 2.1.0, 2026-08-31)

| Benchmark | 2.1.0 | 2.0.1 | 6.3.34 | Iterations |
|-----------|-----|-----|-----|------------|
| voice_count_scaling/1 | 720 ns | 868 ns | 887 ns | 4,410 |
| voice_count_scaling/4 | 1.293 µs | 1.789 µs | 1.875 µs | 4,410 |
| voice_count_scaling/8 | 2.120 µs | 3.105 µs | 3.173 µs | 4,410 |
| voice_count_scaling/16 | 3.644 µs | 5.601 µs | 5.591 µs | 4,410 |
| voice_count_scaling/32 | 6.970 µs | 10.569 µs | 10.689 µs | 4,410 |
| voice_count_scaling/64 | 13.245 µs | 20.614 µs | 20.886 µs | 4,410 |
| fill_buffer_stereo/1 | 364.3 µs | 433.3 µs | 457.7 µs | 40 |
| fill_buffer_stereo/8 | 1.113 ms | 1.555 ms | 1.569 ms | 40 |
| fill_buffer_stereo/16 | 1.901 ms | 2.871 ms | 2.877 ms | 40 |
| fill_buffer_per_sample/1 | 361.4 µs | 429.1 µs | 440.9 µs | 40 |
| fill_buffer_per_sample/8 | 1.080 ms | 1.550 ms | 1.582 ms | 40 |
| fill_buffer_per_sample/16 | 1.904 ms | 2.884 ms | 2.869 ms | 40 |
| interpolation_cubic | **37 ns** | 89 ns | 82 ns | 44,100 |
| interpolation_stereo | **60 ns** | 154 ns | 155 ns | 44,100 |
| fill_buffer_stereo_filtered_8v | **1.511 ms** | 2.247 ms | 2.592 ms | 40 |
| fill_buffer_stereo_filtered_hp_8v | 1.797 ms | *(new 2.0.7)* | — | 40 |
| wsola_1sec_2x | **217.7 ms** | 858.8 ms | 857.1 ms | 2 |

The high-pass case exists to keep an upstream gap visible: naad ships an allocation-free SVF core
for low-pass only, so the other three filter modes still allocate 32 B per channel per sample.
Filed as
[`issues/2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md`](development/issues/2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md).

## Analysis

### Why Cyrius is slower per-operation

| Factor | Cost | Where |
|--------|------|-------|
| f64 vs f32 | ~1.5–2× | all sample math |
| No autovectorization (SIMD) | ~2–4× | mix, interpolation, filter |
| Per-sample heap alloc | — | **Eliminated.** 2.0.2 hoisted the render scratch to per-engine slots and 2.0.7 routed low-pass through naad's alloc-free SVF core; `tests/engine.tcyr` asserts a zero-byte delta across a rendered block. Only high-pass / band-pass / notch still allocate, and that is [an upstream gap](development/issues/2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md). |
| Per-frame block render | ~1.5× | `fill_buffer_stereo` calls `next_sample_stereo` per frame instead of Rust's per-voice block-into-scratch (so it ≈ `fill_buffer_per_sample` and forgoes Rust's ~2.9×/1.2× block speedup). Still open — see the roadmap, where the obvious fix measured *worse* at ≥8 voices. |

The **relative shape holds**: `next_sample_stereo` scales ~linearly with voice count on both
sides (720 ns → 13.2 µs for 1→64), and the filtered path costs ~1.4× the unfiltered 8-voice path
— the same parity signal Rust shows.

### Where Cyrius wins

| Metric | Rust | Cyrius |
|--------|------|--------|
| Precision | f32 (~1e-7) | f64 (~1e-15) |
| Binary | dynamic, many crates | static, self-contained bundle |
| Build | cargo + criterion (minutes) | `cyrius build` (instant) |
| Dependencies | naad + shravan + hisab + hound-era stack | naad/shravan/hisab dist bundles |

### Optimization vectors — status

This section used to say performance was "out of scope until after v2". That stopped being true
in **2.0.7**, which took WSOLA 4.02×, interpolation 2.83× and 64-voice scaling 1.64×, all
bit-identical and verified against a 24,576-sample render differential.

| Lever | Status |
|---|---|
| **Reuse per-sample scratch** | ✅ **Done** — 2.0.2 hoisted the render scratch to per-engine slots (it was 48 B/frame, 2.1 MB/s, never reclaimed); 2.0.7 routed low-pass through naad's alloc-free SVF core; 2.1.0 stopped `note_on` allocating a filter per note (264 → 72 B). `tests/engine.tcyr` pins the zero-byte render delta. |
| **True block render** | ⏳ **Open, and harder than it looked.** The obvious version measured *worse* at ≥8 voices (64-voice low-pass 17.734 → 18.273 ms); the real 2.4× is only the many-idle-slots case. 2.0.7 took most of the per-voice win by other means. Deferred with reasons in the roadmap. |
| **SIMD** | ⏳ Open. Untouched; still worth ~2–4× on mix and interpolation. |
| **Cache filter coeffs** | ✅ Done — the 0.5 Hz cutoff dead-band is ported, and 2.0.7 folded the key-tracking factor into `base_cutoff` at note-on, removing an `f64_pow` per sample per voice. |

Two exact WSOLA transformations (raw-pointer dot product; `prev` as an index rather than a
per-frame copy) delivered the 4.02× on their own. A third — a Cauchy–Schwarz early-exit prune —
was **rejected**: a hand-tuned numerical heuristic, not an exact transformation, trading audio
accuracy for speed against an explicit project rule.

The remaining ~13–16× per-sample ratio is the expected f64/no-SIMD baseline.
