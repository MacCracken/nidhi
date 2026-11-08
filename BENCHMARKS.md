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

## Cyrius current (toolchain 6.7.18, x86_64 Linux, 2026-11-07, nidhi 2.2.1)

naad 2.3.3 · shravan 2.9.3 · hisab 3.4.1. Three trees, minimum of eight rounds each, run in a
rotating order in one session on a quiet machine. The middle column is 2.2.1's source and
dependencies built with cyrius 6.7.9, which isolates the toolchain: its dependency bundles are
byte-identical to 2.2.1's, and only the compiler and its stdlib differ.

| Benchmark | 2.2.0 on 6.7.9 | 2.2.1 on 6.7.9 | 2.2.1 on 6.7.18 | deps + filter | toolchain | net | 2.2.1 faster in |
|---|---|---|---|---|---|---|---|
| `voice_count_scaling/1` | 548 ns | 548 ns | 554 ns | +0.0 % | +1.1 % | +1.1 % | 2 of 8 |
| `voice_count_scaling/4` | 1.01 µs | 968 ns | 1.01 µs | -4.4 % | +4.1 % | -0.5 % | 1 of 8 |
| `voice_count_scaling/8` | 1.64 µs | 1.61 µs | 1.65 µs | -1.4 % | +2.3 % | +0.9 % | 0 of 8 |
| `voice_count_scaling/16` | 2.86 µs | 2.75 µs | 2.93 µs | -3.7 % | +6.4 % | +2.4 % | 0 of 8 |
| `voice_count_scaling/32` | 5.30 µs | 5.30 µs | 5.47 µs | -0.1 % | +3.2 % | +3.1 % | 0 of 8 |
| `voice_count_scaling/64` | 10.24 µs | 10.03 µs | 10.59 µs | -2.0 % | +5.6 % | +3.5 % | 0 of 8 |
| `fill_buffer_stereo/1` | 287.06 µs | 284.32 µs | 293.10 µs | -1.0 % | +3.1 % | +2.1 % | 1 of 8 |
| `fill_buffer_stereo/8` | 904.40 µs | 874.38 µs | 931.07 µs | -3.3 % | +6.5 % | +2.9 % | 1 of 8 |
| `fill_buffer_stereo/16` | 1.612 ms | 1.565 ms | 1.645 ms | -2.9 % | +5.1 % | +2.0 % | 1 of 8 |
| `fill_buffer_per_sample/1` | 286.31 µs | 283.00 µs | 292.56 µs | -1.2 % | +3.4 % | +2.2 % | 2 of 8 |
| `fill_buffer_per_sample/8` | 905.47 µs | 881.01 µs | 929.21 µs | -2.7 % | +5.5 % | +2.6 % | 1 of 8 |
| `fill_buffer_per_sample/16` | 1.601 ms | 1.559 ms | 1.651 ms | -2.6 % | +5.9 % | +3.1 % | 1 of 8 |
| `interpolation_cubic` | 34 ns | 33 ns | 34 ns | -2.9 % | +3.0 % | +0.0 % | 5 of 8 |
| `interpolation_stereo` | 57 ns | 56 ns | 56 ns | -1.8 % | +0.0 % | -1.8 % | 3 of 8 |
| `fill_buffer_stereo_filtered_8v` | 1.346 ms | 1.323 ms | 1.409 ms | -1.7 % | +6.5 % | +4.7 % | 2 of 8 |
| `fill_buffer_stereo_filtered_hp_8v` | 1.569 ms | 1.457 ms | 1.502 ms | -7.1 % | +3.1 % | -4.3 % | 8 of 8 |
| `wsola_1sec_2x` | 227.894 ms | 226.438 ms | 227.721 ms | -0.6 % | +0.6 % | -0.1 % | 6 of 8 |

**The dependencies and the filter change make 2.2.1 faster, and the 6.7.18 toolchain takes more than
that back on most render paths.** On 6.7.9, 2.2.1 is 0 to 7.1 % faster than 2.2.0. Built with
6.7.18, the same source is 1.1 to 6.5 % slower than that on every `voice_count_scaling` and
`fill_buffer*` row. Net against 2.2.0 those rows run −0.5 % to +4.7 %, and 2.2.1 is faster in at
most 2 of 8 rounds. The exceptions are high-pass, 4.3 % faster in all 8 rounds
because it no longer allocates, and `wsola_1sec_2x`, flat. The stdlib changes between the two
snapshots on these paths are cold (`vec`'s abort messages) or unused here (`math`'s float parsing
tables, `lcm`). That points at 6.7.18's code generation or layout, not narrowed further. An earlier
two-tree run of eight interleaved pairs agrees: low-pass filtered +6.0 %, high-pass −3.9 %.
Output is byte-identical to 2.2.0.

## Cyrius 2.2.0 (toolchain 6.7.9, x86_64 Linux, 2026-11-07)

naad 2.3.0 · shravan 2.9.0 · hisab 3.4.0. Minimum of eight interleaved runs per tree, same session,
on a quiet machine (run-to-run spread 2–19 %). These line up with the August figures far better
than the 2.1.3 table below, which was measured on a loaded machine.

| Benchmark | 6.6.20 (2.1.3) | 6.7.9 (2.2.0) | Δ min | Δ median | 6.7.9 slower in |
|---|---|---|---|---|---|
| `voice_count_scaling/1` | 567 ns | 564 ns | -0.5 % | -2.1 % | 3 of 8 |
| `voice_count_scaling/4` | 1.03 µs | 1.03 µs | -0.1 % | -0.3 % | 6 of 8 |
| `voice_count_scaling/8` | 1.69 µs | 1.68 µs | -0.1 % | -0.8 % | 4 of 8 |
| `voice_count_scaling/16` | 2.97 µs | 2.98 µs | +0.2 % | +0.6 % | 5 of 8 |
| `voice_count_scaling/32` | 5.56 µs | 5.57 µs | +0.2 % | +1.0 % | 6 of 8 |
| `voice_count_scaling/64` | 10.67 µs | 10.64 µs | -0.2 % | +0.5 % | 6 of 8 |
| `fill_buffer_stereo/1` | 302.08 µs | 301.54 µs | -0.2 % | +0.7 % | 7 of 8 |
| `fill_buffer_stereo/8` | 941.21 µs | 935.11 µs | -0.6 % | +2.0 % | 6 of 8 |
| `fill_buffer_stereo/16` | 1.673 ms | 1.661 ms | -0.7 % | +0.3 % | 6 of 8 |
| `fill_buffer_per_sample/1` | 301.37 µs | 299.41 µs | -0.7 % | -0.6 % | 2 of 8 |
| `fill_buffer_per_sample/8` | 940.61 µs | 943.95 µs | +0.4 % | +0.7 % | 5 of 8 |
| `fill_buffer_per_sample/16` | 1.674 ms | 1.652 ms | -1.3 % | +0.5 % | 5 of 8 |
| `interpolation_cubic` | 36 ns | 36 ns | +0.0 % | +0.0 % | 4 of 8 |
| `interpolation_stereo` | 60 ns | 59 ns | -1.7 % | -1.6 % | 2 of 8 |
| `fill_buffer_stereo_filtered_8v` | 1.439 ms | 1.386 ms | -3.7 % | -0.8 % | 2 of 8 |
| `fill_buffer_stereo_filtered_hp_8v` | 1.658 ms | 1.606 ms | -3.1 % | +1.1 % | 5 of 8 |
| `wsola_1sec_2x` | 240.935 ms | 235.458 ms | -2.3 % | -1.5 % | 2 of 8 |

**Flat.** The minimums move between −3.7 % and +0.4 %, and every benchmark's ranges overlap. The
first three runs had all six `voice_count_scaling` cases at +3–5 %; five more runs took them to
−0.5 % to +0.2 %, so that was noise. Output is byte-identical to 2.1.3.

## Cyrius 2.1.3 (toolchain 6.6.20, x86_64 Linux, 2026-11-07)

naad 2.2.5 · shravan 2.8.2 · hisab 3.3.7. Best of three alternating runs per tree, same machine,
same session. **Compare within this table only:** this machine read about twice the August figures
below on both toolchains that day, so the absolute numbers do not line up with the older sections.

| Benchmark | 6.6.3 (2.1.2) | 6.6.20 (2.1.3) | Δ | Spread on 6.6.20 |
|---|---|---|---|---|
| `voice_count_scaling/1` | 1.23 µs | 1.28 µs | +3.6 % | 48 % |
| `voice_count_scaling/4` | 3.04 µs | 2.32 µs | -23.7 % | 57 % |
| `voice_count_scaling/8` | 5.28 µs | 3.93 µs | -25.6 % | 30 % |
| `voice_count_scaling/16` | 7.49 µs | 6.96 µs | -7.2 % | 24 % |
| `voice_count_scaling/32` | 13.18 µs | 12.88 µs | -2.3 % | 27 % |
| `voice_count_scaling/64` | 25.94 µs | 24.69 µs | -4.8 % | 30 % |
| `fill_buffer_stereo/1` | 683.59 µs | 669.58 µs | -2.0 % | 36 % |
| `fill_buffer_stereo/8` | 2.049 ms | 2.131 ms | +4.0 % | 32 % |
| `fill_buffer_stereo/16` | 3.715 ms | 3.801 ms | +2.3 % | 36 % |
| `fill_buffer_per_sample/1` | 683.39 µs | 669.72 µs | -2.0 % | 36 % |
| `fill_buffer_per_sample/8` | 2.073 ms | 2.124 ms | +2.5 % | 35 % |
| `fill_buffer_per_sample/16` | 3.618 ms | 3.762 ms | +4.0 % | 35 % |
| `interpolation_cubic` | 77 ns | 76 ns | -1.3 % | 8 % |
| `interpolation_stereo` | 128 ns | 122 ns | -4.7 % | 50 % |
| `fill_buffer_stereo_filtered_8v` | 3.131 ms | 3.349 ms | +7.0 % | 30 % |
| `fill_buffer_stereo_filtered_hp_8v` | 3.501 ms | 3.865 ms | +10.4 % | 27 % |
| `wsola_1sec_2x` | 522.435 ms | 531.510 ms | +1.7 % | 27 % |

**No measurable change.** The runs spread 2–67 % within a single tree, and every Δ sits inside that.
`interpolation_cubic` and `interpolation_stereo`, the paths that gained the 2.1.3 position guard,
read −1.3 % and −4.7 %. The filtered 8-voice cases read +7–10 % with 27–42 % spread: not evidence of
a regression, and not evidence against one. A quiet machine would settle it.

Output is **not** bit-identical to 2.1.2. See the 2.1.3 changelog entry.

## Cyrius 2.0.7 (toolchain 6.5.36, x86_64 Linux, 2026-08-31)

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
