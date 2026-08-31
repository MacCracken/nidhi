# nidhi benchmarks — Cyrius vs Rust

The Cyrius port reproduces the 7 Rust Criterion benchmarks (`rust-old/benches/benchmarks.rs`)
as `tests/nidhi.bcyr`, measuring the **same operations** so the two implementations can be
compared head-to-head.

## Run

```sh
# Cyrius port:
cyrius bench tests/nidhi.bcyr

# Rust original (needs the Rust toolchain + published naad/shravan/hisab):
cd rust-old && cargo bench
```

Cyrius batches N iterations between one `clock_gettime` pair and reports the per-iteration
average (`bench_batch_start` → tight loop → `bench_batch_stop(N)` → `bench_report`). Criterion
reports a statistical estimate per call. Compare the **per-operation** figures.

## Cyrius current (toolchain 6.5.36, x86_64 Linux, 2026-08-31)

naad 2.2.2 · shravan 2.8.0 · hisab 2.11.2.

| Benchmark | Operation | Cyrius avg | vs 6.3.34 |
|---|---|---:|---:|
| `voice_count_scaling/1` | `next_sample_stereo`, 1 voice | 868 ns | −2.1 % |
| `voice_count_scaling/4` | 4 voices | 1.789 µs | −4.6 % |
| `voice_count_scaling/8` | 8 voices | 3.105 µs | −2.1 % |
| `voice_count_scaling/16` | 16 voices | 5.601 µs | +0.2 % |
| `voice_count_scaling/32` | 32 voices | 10.569 µs | −1.1 % |
| `voice_count_scaling/64` | 64 voices | 20.614 µs | −1.3 % |
| `fill_buffer_stereo/1` | 512-frame block, 1 voice | 433.3 µs | −5.3 % |
| `fill_buffer_stereo/8` | 8 voices | 1.555 ms | −0.9 % |
| `fill_buffer_stereo/16` | 16 voices | 2.871 ms | −0.2 % |
| `fill_buffer_per_sample/1` | 512-frame per-sample loop, 1 voice | 429.1 µs | −2.7 % |
| `fill_buffer_per_sample/8` | 8 voices | 1.550 ms | −2.0 % |
| `fill_buffer_per_sample/16` | 16 voices | 2.884 ms | +0.5 % |
| `interpolation_cubic` | `read_cubic` (mono) | 89 ns | +8.5 % |
| `interpolation_stereo` | `read_stereo_interpolated` | 154 ns | −0.6 % |
| `fill_buffer_stereo_filtered_8v` | 512-frame block, 8 filtered voices | 2.247 ms | **−13.3 %** |
| `wsola_1sec_2x` | WSOLA stretch 2× on 1 s | 858.8 ms | +0.2 % |

Every figure is within run-to-run noise of the 6.3.34 baseline except
`fill_buffer_stereo_filtered_8v`, which is **~12–13 % faster** and reproduced across three runs
(2.247 / 2.289 / 2.313 ms vs a 2.592 ms baseline) — the filtered render path is the one that
leans hardest on naad's SVF.

The `interpolation_cubic` column above is **not** a regression: the table records the single run
appended to `bench-history.csv`, and repeat runs of the same binary landed at 81 ns and 82 ns.
On an ~82 ns operation measured against a ~1.33 µs timer floor, one quantum of jitter swamps the
signal. Treat sub-200 ns rows as indicative only.

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

History rows are appended to `bench-history.csv` (gitignored by the inherited `.gitignore`;
un-ignore it if you want the series tracked in git, as the sibling Cyrius repos do).
