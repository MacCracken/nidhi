# nidhi — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**2.0.4** — 2026-08-31. The Rust oracle is retired: golden vectors captured, build identity
recorded, `rust-old/` removed. Follows 2.0.3 (crossfade seam), 2.0.2 (P-1 sweep), 2.0.1
(toolchain/dependency catch-up) and 2.0.0 (the port).

## Toolchain

- **Cyrius pin**: `6.5.36` (in `cyrius.cyml [package].cyrius` — the single source of truth; CI
  reads it rather than hardcoding a version)

## Source

- Rust reference: **retired in 2.0.4.** The 7180-line crate lives in git history
  (`git show <rev>:rust-old/src/<mod>.rs`); its build identity and full dependency lockfile are
  recorded at [`docs/port/oracle-build-identity.md`](../port/oracle-build-identity.md), and the
  behaviour it pinned is asserted by `tests/golden.tcyr`.
- Cyrius port: **all 14 modules ported**, `src/*.cyr`, listed in dependency order under
  `cyrius.cyml [lib].modules`:
  `error` → `f64_util` → `loop_mode` → `envelope` → `zone` → `sample` → `instrument` →
  `capture` → `stretch` → `effect_chain` → `io` → `sf2` → `sfz` → `engine`.
- `[build].entry` is `programs/smoke.cyr` — a build-chain smoke test, not the product. nidhi is a
  library; consumers take `dist/nidhi.cyr` (built by `cyrius distlib`).

## Tests

- **14 suites / 398 assertions / 0 failures** (`cyrius test`), zero `#must_use` warnings
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

Every open item is now pinned to a release in [`roadmap.md`](roadmap.md). The ones that would
bite a consumer first:

- **`n_effect_apply` dispatches on the slot tag, not the state variant** — retagging a live slot
  hands the wrong struct to a naad processor. Pinned to 2.0.6.
- **`#derive(accessors)` generates public setters that bypass every clamp** Rust enforced by
  keeping the fields private. Pinned to 2.0.6.
- **High-pass / band-pass / notch voices still allocate** ~180 MB/s at 64 voices; naad ships an
  alloc-free core for low-pass only. Pinned to 2.1.0, gated on naad.
- **The serde claim is false.** CLAUDE.md and three port docs say config types carry
  `#derive(Serialize)`; none does. Pinned to 2.0.6 — implement or document the drop.
- **Stretch output *values* are untested on both sides** — all 13 Rust and all 40 Cyrius stretch
  assertions check only length/finiteness. Golden vectors pinned to 2.0.5.

## rust-old/ — retired (2.0.4)

Removed from the working tree; **377 MB reclaimed** (repo 525 MB → 148 MB). Nothing was lost:

- The 1002 tracked files are in git history — `git show HEAD~1:rust-old/src/engine.rs` works.
- The two files that were **not** tracked, and would have been lost, are preserved:
  `Cargo.lock` (all 88 dependency versions) and the rustc identity, both in
  [`docs/port/oracle-build-identity.md`](../port/oracle-build-identity.md);
  `bench-history-rust.csv` is now tracked (the blanket `*.csv` ignore was excluding it *and* the
  live benchmark series).
- The behaviour it pinned is asserted by `tests/golden.tcyr` — 43 assertions against values
  captured from a live oracle build, closing the gap the 2.0.3 audit named as the single largest
  untested surface in the port (stretch output *values*, previously untested on **both** sides).

## Next

See [`roadmap.md`](roadmap.md).
