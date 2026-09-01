# nidhi — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**2.0.2** — 2026-08-31. P-1 audit and repair sweep on top of the 2.0.1 toolchain/dependency
catch-up and the 2.0.0 Rust → Cyrius port (2026-07-03). 7180 lines of Rust preserved at
`rust-old/` as the parity oracle.

## Toolchain

- **Cyrius pin**: `6.5.36` (in `cyrius.cyml [package].cyrius` — the single source of truth; CI
  reads it rather than hardcoding a version)

## Source

- Rust reference: 7180 lines at `rust-old/` (frozen, do not edit).
- Cyrius port: **all 14 modules ported**, `src/*.cyr`, listed in dependency order under
  `cyrius.cyml [lib].modules`:
  `error` → `f64_util` → `loop_mode` → `envelope` → `zone` → `sample` → `instrument` →
  `capture` → `stretch` → `effect_chain` → `io` → `sf2` → `sfz` → `engine`.
- `[build].entry` is `programs/smoke.cyr` — a build-chain smoke test, not the product. nidhi is a
  library; consumers take `dist/nidhi.cyr` (built by `cyrius distlib`).

## Tests

- **14 suites / 378 assertions / 0 failures** (`cyrius test`), zero `#must_use` warnings
- Render path asserted **allocation-free**: `alloc_used()` delta of 0 across 20 blocks at 8 and
  64 voices, filtered and unfiltered (`tests/engine.tcyr`)
- **Fuzz**: `fuzz/fuzz_sf2.fcyr` + `fuzz/fuzz_sfz.fcyr` — 2 passed, 0 crashes
- **Benchmarks**: `tests/nidhi.bcyr`, 16 measurements reproducing the 7 Rust criterion
  benchmarks. See [`BENCHMARKS.md`](../../BENCHMARKS.md) and
  [`bench-history.csv`](../../bench-history.csv).

## Dependencies

Direct (declared in `cyrius.cyml`):

- **stdlib** — string, fmt, alloc, vec, str, syscalls, io, args, assert, bench, tagged, result,
  fnptr, math, ganita, hashmap, bayan
- **naad** 2.2.2 — DSP (SVF filters, ADSR, LFOs, effects, voice management, interpolation)
- **shravan** 2.8.0 — audio codecs (WAV decode/encode + streaming), behind `src/io.cyr`
- **hisab** 2.11.2 — math

Transitive, resolved into `lib/`: **goonj** 2.0.4, **sankoch** 2.7.10, **sakshi** 2.4.11.

## Consumers

**dhvani** (audio engine), and thereby **shruti** (DAW) — nidhi replaces
`shruti-instruments::sampler`. Both vendor `dist/nidhi.cyr`.

## Known hazards

_(Everything listed here at 2.0.1 — the `ERR_NONE` collision, the `src/main.cyr` CI break, the
sf2 magic literals, and the integer-PCM silence — is resolved in 2.0.2.)_

Open, deferred from the 2.0.2 sweep with reasons in CHANGELOG:

- **Loop crossfade never closes the seam.** `xfade_pos = loop_start + (xfade - dist)` splices
  from the wrong source position, and `crossfade_length` is never clamped against the loop
  length. Both are character-identical to rust-old, both are output-changing, and there is **no
  crossfade test or benchmark today** — `grep -n crossfade tests/*.tcyr` returns nothing.
- **High-pass / band-pass / notch voices still allocate** ~180 MB/s at 64 voices. naad 2.2.2
  ships an alloc-free core for low-pass only; the 4-output equivalent needs a naad change.
- **`NZone_volume_db` is parsed, inherited, stored, and never read.** A `volume=-6` region
  renders at full level. Oracle-faithful, but a field that looks wired up and is not.
- **100 top-level names skip the `n_`/`N` prefix** (the `sf2_*`/`Sf2*` surface, `sfz_*` helpers,
  and the bare `CC_*`/`FX_*`/`LOOP_*` constant families). Zero measured collisions today.

## Next

See [`roadmap.md`](roadmap.md).
