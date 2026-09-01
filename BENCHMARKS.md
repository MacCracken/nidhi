# nidhi benchmarks — Cyrius vs Rust

The Cyrius port reproduces the 7 Rust Criterion benchmarks (`rust-old/benches/benchmarks.rs`)
as `tests/nidhi.bcyr`, measuring the **same operations** so the two implementations can be
compared head-to-head.

## Run

```sh
# Cyrius port:
cyrius bench tests/nidhi.bcyr

# Rust original: NOT reproducible from the working tree -- rust-old/ was retired in 2.0.4.
# Its numbers survive in bench-history-rust.csv. To re-measure, restore the crate from git
# history and see docs/port/oracle-build-identity.md for the exact deps and rustc.
#   git log --all -- rust-old/            # find the last revision that had it
```

Cyrius batches N iterations between one `clock_gettime` pair and reports the per-iteration
average (`bench_batch_start` → tight loop → `bench_batch_stop(N)` → `bench_report`). Criterion
reports a statistical estimate per call. Compare the **per-operation** figures.

## Cyrius current (toolchain 6.5.36, x86_64 Linux, 2026-08-31, nidhi 2.0.7)

naad 2.2.2 · shravan 2.8.0 · hisab 2.11.2. **Every 2.0.7 change is bit-identical** — verified by
a render differential over 24,576 samples across 8 configurations (mono/stereo source, forward
loop, crossfaded loop, low-pass, high-pass, pitched, key-tracked), plus the golden vectors in
`tests/golden.tcyr` for the WSOLA path.

| Benchmark | 2.0.7 | 2.0.6 | 6.3.34 baseline | 2.0.6 → 2.0.7 |
|---|---:|---:|---:|---:|
| `voice_count_scaling/1` | 720 ns | 893 ns | 887 ns | **1.24×** |
| `voice_count_scaling/4` | 1.293 µs | 1.926 µs | 1.875 µs | **1.49×** |
| `voice_count_scaling/8` | 2.120 µs | 3.194 µs | 3.173 µs | **1.51×** |
| `voice_count_scaling/16` | 3.644 µs | 5.829 µs | 5.591 µs | **1.60×** |
| `voice_count_scaling/32` | 6.970 µs | 11.069 µs | 10.689 µs | **1.59×** |
| `voice_count_scaling/64` | 13.245 µs | 21.761 µs | 20.886 µs | **1.64×** |
| `fill_buffer_stereo/1` | 364.3 µs | 442.8 µs | 457.7 µs | 1.22× |
| `fill_buffer_stereo/8` | 1.113 ms | 1.626 ms | 1.569 ms | **1.46×** |
| `fill_buffer_stereo/16` | 1.901 ms | 2.945 ms | 2.877 ms | **1.55×** |
| `fill_buffer_per_sample/1` | 361.4 µs | 433.4 µs | 440.9 µs | 1.20× |
| `fill_buffer_per_sample/8` | 1.080 ms | 1.614 ms | 1.582 ms | **1.49×** |
| `fill_buffer_per_sample/16` | 1.904 ms | 2.924 ms | 2.869 ms | **1.54×** |
| `interpolation_cubic` | 37 ns | 87 ns | 82 ns | **2.35×** |
| `interpolation_stereo` | 60 ns | 170 ns | 155 ns | **2.83×** |
| `fill_buffer_stereo_filtered_8v` | 1.511 ms | 2.063 ms | 2.592 ms | **1.37×** |
| `fill_buffer_stereo_filtered_hp_8v` | 1.797 ms | *(new)* | — | — |
| `wsola_1sec_2x` | **217.7 ms** | 874.7 ms | 857.1 ms | **4.02×** |

### Where the time went

**WSOLA, 4.02×** — the correlation search evaluates ~88M element pairs for a one-second stretch,
and every one went through `vec_get`'s two bounds tests, twice. It now uses a raw-pointer dot
product, and `prev` became an *index* into `input` rather than a fresh frame-size copy per frame
(it was always a pure slice, and the loop guard already proved the span in range) — which also
stops ~2.8 MB of never-reclaimed allocation per stretch.

**Interpolation, 2.35–2.83×** — the four taps span `idx-1 .. idx+2`, so one interior test
(`idx - 1 >= 0 && idx + 2 < frames`) proves all of them in range, for stereo too. That retires
eight per-tap helper calls with their own bounds tests and accessor calls in favour of eight raw
loads. Zero-padding semantics stay on the edge path. The hermite coefficients are now literal
bit patterns instead of two divides and three negations per call.

**Voice scaling, up to 1.64×** — mostly the interpolation win compounding per voice, plus the
filter key-tracking factor folded into `base_cutoff` at note-on (its two inputs are fixed for the
voice's life, so an `f64_pow` per sample per voice was pure waste) and the literal hoists in
`nvf_set_cutoff`.

**Real-time headroom**: `fill_buffer_stereo/16` is 1.901 ms against a 512-frame budget of
11.6 ms at 44.1 kHz — **6.1× headroom**, up from 3.9×. 64 voices sit at 13.245 µs per
`next_sample_stereo`.

### The high-pass case is new, and it is there to be uncomfortable

`fill_buffer_stereo_filtered_hp_8v` (1.797 ms) runs the same workload as
`fill_buffer_stereo_filtered_8v` (1.511 ms) with `FILTER_HIGHPASS`. The **19 % penalty** is naad
shipping an allocation-free SVF core for low-pass only; high-pass, band-pass and notch voices
still take the allocating one-shot path (~180 MB/s at 64 voices). Every other benchmark in the
file sets `FILTER_LOWPASS`, so this gap was invisible. Filed in naad's roadmap. Not strictly blocking: naad's `filter_biquad_process_sample` is an allocation-free alternative covering all three modes, at the cost of a different topology.

## Cyrius baseline (toolchain 6.3.34, x86_64 Linux, 2026-07-02)

| Benchmark | Operation | Cyrius avg |
|---|---|---:|
| `voice_count_scaling/1` | `next_sample_stereo`, 1 voice | 887 ns |
| `voice_count_scaling/4` | 4 voices | 1.875 µs |
| `voice_count_scaling/8` | 8 voices | 3.173 µs |
| `voice_count_scaling/16` | 16 voices | 5.591 µs |
| `voice_count_scaling/32` | 32 voices | 10.689 µs |
| `voice_count_scaling/64` | 64 voices | 20.886 µs |
| `fill_buffer_stereo/1` | 512-frame block, 1 voice | 457.7 µs |
| `fill_buffer_stereo/8` | 8 voices | 1.569 ms |
| `fill_buffer_stereo/16` | 16 voices | 2.877 ms |
| `fill_buffer_per_sample/1` | 512-frame per-sample loop, 1 voice | 440.9 µs |
| `fill_buffer_per_sample/8` | 8 voices | 1.582 ms |
| `fill_buffer_per_sample/16` | 16 voices | 2.869 ms |
| `interpolation_cubic` | `read_cubic` (mono) | 82 ns |
| `interpolation_stereo` | `read_stereo_interpolated` | 155 ns |
| `fill_buffer_stereo_filtered_8v` | 512-frame block, 8 filtered voices | 2.592 ms |
| `wsola_1sec_2x` | WSOLA stretch 2× on 1 s | 857.1 ms |

## Notes on parity of the comparison

- **`next_sample_stereo` scales ~linearly** with voice count (887 ns → 20.9 µs for 1→64
  voices), as expected — each voice is an independent interpolate → filter → envelope → mix.
- **`fill_buffer_stereo` ≈ `fill_buffer_per_sample`** in Cyrius: the Cyrius port renders the
  block per-frame (output-identical to the Rust per-voice block render, but without Rust's
  cache-locality restructuring), so the two paths measure nearly the same cost here. This is a
  known, deliberate simplification (see `docs/port/01-PLAN.md`).
- Expect the Cyrius figures to be **slower than Rust in absolute ns** (the ecosystem reference
  is ~40× on hot f64 loops, since Cyrius is f64-only and does no autovectorization, and the
  SVF/reverb allocate an output struct per sample). The **relative shape** across benchmarks is
  the parity signal, not the absolute nanoseconds.
- `wsola_1sec_2x` is dominated by the O(frames × tolerance × frame_size) cross-correlation
  search — the most Cyrius-vs-Rust-divergent case.

History rows are appended to `bench-history.csv`, which is tracked as of 2.0.4 — the inherited
`.gitignore` excluded it via a blanket `*.csv`, and that also excluded `bench-history-rust.csv`,
now the only surviving record of the Rust oracle's numbers.
