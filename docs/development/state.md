# nidhi — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**2.0.1** — 2026-08-31. Toolchain + dependency catch-up on top of the 2.0.0 Rust → Cyrius port
(2026-07-03). 7180 lines of Rust preserved at `rust-old/` as the parity oracle.

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
  library; consumers take `dist/nidhi.cyr` (3695 lines, built by `cyrius distlib`).

## Tests

- **14 suites / 313 assertions / 0 failures** (`cyrius test`)
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

- **`ERR_NONE` collides with `lib/goonj.cyr`** in the flat bundle namespace. Both define it as
  `0`, so nothing misbehaves today — but this is precisely the invisible-until-one-side-renumbers
  hazard naad 2.2.0 renamed its own `ERR_*` to `NAAD_ERR_*` to escape. Renaming nidhi's public
  error constants is a breaking change, so it is deferred to the next minor.
- `.github/workflows/ci.yml` still builds `src/main.cyr`, a scaffold path that does not exist in
  this repo (the entry is `programs/smoke.cyr`). Pre-existing; not introduced by 2.0.1.

## Next

See [`roadmap.md`](roadmap.md).
