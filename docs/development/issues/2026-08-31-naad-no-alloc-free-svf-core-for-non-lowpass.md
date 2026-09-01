# 2026-08-31 — naad: no allocation-free SVF core for high-pass / band-pass / notch

**Component:** `naad` — `src/filter.cyr`, the `StateVariableFilter` sample path.
**naad version seen:** **2.2.2** (`cyrius.cyml [deps.naad] tag = "2.2.2"`).
**Severity:** **Medium.** Nothing is mis-computed. The cost is unreclaimable heap on a real-time
audio path, which on a bump allocator is eventually fatal rather than merely slow.
**nidhi impact:** every voice using `FILTER_HIGHPASS`, `FILTER_BANDPASS` or `FILTER_NOTCH`
allocates **32 bytes per channel per sample**. At 64 voices that is **180.6 MB/s**, none of it
reclaimable. Low-pass voices — the default, and the SFZ default — are already unaffected.

## Request

Export an allocation-free four-output core, mirroring the `reverb_process_core` pattern naad
already uses:

```
fn _filter_svf_compute_into(self, input, out4)   # writes low/high/band/notch into out4
```

`out4` being caller-owned scratch of `sizeof(SvfOutput)`, so the caller can reuse one slot for
the life of a voice. A public wrapper name is fine too; the shape is what matters.

## Why this is a naad change and not a nidhi one

naad **already has this exact pattern twice**, so the request is to complete it, not to introduce
it:

- `reverb_process_core(self, input, res)` — out-param core, with `reverb_process_sample` as a
  three-line allocating wrapper over it (`lib/naad.cyr:4786` / `:4845`).
- `_filter_svf_compute_lowpass(self, input)` — value-returning, allocation-free, and its own
  header says *"Shared by the low-pass sample/buffer hot paths so the bump allocator does not
  grow per sample. Numerics are bit-identical to filter_svf_process_sample's low_pass field."*

That comment is the whole argument: the low-pass hot path was given an escape hatch and the other
three modes were not. nidhi took the low-pass one in 2.0.7 and measured a **19 %** block-render
improvement from it.

Inlining the Cytomic/Simper core into nidhi would fix nidhi and fork DSP that CLAUDE.md says must
come from naad, so nidhi is deliberately **not** doing that.

## Measurement

Directly measured against naad 2.2.2, `alloc_used()` around 44,100 calls on one filter:

| path | bytes / 44,100 calls | per call |
|---|---:|---:|
| `filter_svf_process_sample` (all four outputs) | 1,411,200 | **32** |
| `filter_svf_process_sample_lowpass` | **0** | 0 |

32 B/call is exactly `sizeof(SvfOutput)` — `struct SvfOutput { low_pass; high_pass; band_pass;
notch; }`, four f64s.

Scaled to a full engine: 64 voices × 2 channels × 44,100 Hz = 5,644,800 calls/s × 32 B =
**180,633,600 B/s**.

End-to-end, on identical 8-voice 512-frame block renders differing only in filter type
(`tests/nidhi.bcyr`, nidhi 2.1.0):

| benchmark | time |
|---|---:|
| `fill_buffer_stereo_filtered_8v` (low-pass, uses the alloc-free core) | **1.528 ms** |
| `fill_buffer_stereo_filtered_hp_8v` (high-pass, allocating one-shot) | **1.819 ms** |

**19 % penalty**, attributable to the allocation, since the two workloads are otherwise identical.

## Why the allocation is worse than it looks

`lib/alloc.cyr` is a **bump allocator whose free is a no-op**. Allocation on the render path is
therefore not a throughput cost that a fast allocator would absorb — it is unbounded growth for
the life of the process, ending in `alloc()` returning 0 inside the audio callback. nidhi spent
2.0.2 removing every other per-sample allocation from its render path for exactly this reason and
now asserts a zero-byte delta over a rendered block (`tests/engine.tcyr`, "render path is
allocation-free"). This is the one remaining source, and it is not on nidhi's side of the line.

## Workaround in place

nidhi 2.0.7 routes `FILTER_LOWPASS` voices — the default, and the SFZ default — through
`filter_svf_process_sample_lowpass`, and leaves the other three modes on the allocating path
(`src/engine.cyr`, `nvf_process_stereo`). `tests/nidhi.bcyr` carries
`fill_buffer_stereo_filtered_hp_8v` specifically so the remaining gap stays visible rather than
being invisible behind an all-low-pass benchmark set.

## Acceptance

nidhi can route all four modes through the new core, delete the mode branch in
`nvf_process_stereo`, and extend its existing zero-allocation assertion to cover high-pass,
band-pass and notch. Numerics must stay bit-identical to the current
`filter_svf_process_sample` fields — nidhi verifies render output byte-for-byte against a
24,576-sample differential, so any drift will show up immediately.
