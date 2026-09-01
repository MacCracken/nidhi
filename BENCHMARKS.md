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

## Cyrius current (toolchain 6.5.36, x86_64 Linux, 2026-08-31, nidhi 2.0.2)

naad 2.2.2 · shravan 2.8.0 · hisab 2.11.2. Two runs shown where they differ meaningfully.

| Benchmark | 2.0.2 | 2.0.1 | 6.3.34 baseline |
|---|---:|---:|---:|
| `voice_count_scaling/1` | 839 ns | 868 ns | 887 ns |
| `voice_count_scaling/4` | 1.719 µs | 1.789 µs | 1.875 µs |
| `voice_count_scaling/8` | 2.975 µs | 3.105 µs | 3.173 µs |
| `voice_count_scaling/16` | 5.480 µs | 5.601 µs | 5.591 µs |
| `voice_count_scaling/32` | 10.524 µs | 10.569 µs | 10.689 µs |
| `voice_count_scaling/64` | 21.038 µs | 20.614 µs | 20.886 µs |
| `fill_buffer_stereo/1` | 420.3 µs | 433.3 µs | 457.7 µs |
| `fill_buffer_stereo/8` | 1.532 ms | 1.555 ms | 1.569 ms |
| `fill_buffer_stereo/16` | 2.814 ms | 2.871 ms | 2.877 ms |
| `fill_buffer_per_sample/1` | 431.2 µs | 429.1 µs | 440.9 µs |
| `fill_buffer_per_sample/8` | 1.543 ms | 1.550 ms | 1.582 ms |
| `fill_buffer_per_sample/16` | 2.822 ms | 2.884 ms | 2.869 ms |
| `interpolation_cubic` | 83 ns | 89 ns | 82 ns |
| `interpolation_stereo` | 156 ns | 154 ns | 155 ns |
| `fill_buffer_stereo_filtered_8v` | **1.976–2.021 ms** | 2.247 ms | 2.592 ms |
| `wsola_1sec_2x` | 850.3 ms | 858.8 ms | 857.1 ms |

**`fill_buffer_stereo_filtered_8v` is the one real movement: 2.592 → ~2.0 ms, about 22 %
cumulative.** Roughly half came in 2.0.1 from the naad 2.2.2 SVF, and the rest in 2.0.2 from
routing low-pass voices through naad's allocation-free `filter_svf_process_sample_lowpass`
instead of the one-shot API that allocates an `SvfOutput` per channel per voice per sample.
The benchmark sets `FILTER_LOWPASS`, so it sees the whole win; high-pass, band-pass and notch
voices still take the allocating path until naad ships an alloc-free 4-output core.

**The render path now allocates zero bytes.** `tests/engine.tcyr` asserts an `alloc_used()`
delta of exactly 0 across 20 blocks at 8 and 64 voices, filtered and unfiltered. Before 2.0.2 it
was 48 B/frame — 2,116,800 B/s at 44.1 kHz — against a bump allocator whose free is a no-op.
That never showed up in these numbers because the benchmarks are far too short to hit the
ceiling; it is a soak-time process death, not a throughput cost.

Everything else is within run-to-run noise. Sub-200 ns rows (`interpolation_cubic`,
`interpolation_stereo`) are measured against a ~1.3 µs timer floor and move by ±8 ns between
runs of the same binary — read them as indicative only.

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
