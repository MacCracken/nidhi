# nidhi — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**2.1.1** — 2026-08-31. `note_on` allocates nothing (264 B -> 72 B -> 0), and a reused voice no
longer inherits the previous note's cutoff. Follows 2.1.0 (zone volume applied, streaming reader),
2.0.7 (performance), 2.0.6 (latent hazards), 2.0.5 (coverage backfill), 2.0.4 (oracle retired),
2.0.3 (crossfade seam), 2.0.2 (P-1 sweep), 2.0.1 (toolchain catch-up), 2.0.0 (the port).

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

- **15 suites / 573 assertions / 0 failures** (`cyrius test`), zero `#must_use` warnings
- Render path **and `note_on`** asserted allocation-free: `alloc_used()` delta of 0 across 20
  blocks at 8 and 64 voices, and 0 bytes per note_on/note_off pair (`tests/engine.tcyr`)
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

Everything is pinned in [`roadmap.md`](roadmap.md); this is the short list of what a consumer
could actually hit.

- **Per-zone bus routing does not work.** `output=` is parsed from SFZ, inherited and stored,
  and **read by nothing** — the same shape `volume_db` had before 2.1.0. Pinned to 2.1.1. The
  README says so too rather than listing it as a feature.
- **High-pass / band-pass / notch voices allocate ~180 MB/s at 64 voices** — and this allocator
  never frees (see [architecture 001](../architecture/001-the-allocator-never-frees.md)).
  Low-pass, the default, is allocation-free. Filed in **naad's** roadmap. Not blocking: naad's
  `filter_biquad_process_sample` is an allocation-free alternative covering all three modes.
- **WSOLA amplifies output up to ~588x** where `window_sum` falls under the `1e-6` normalise
  threshold. **Inherited from the oracle**, identical threshold and guard — a port-faithful
  defect, not a port defect. Fixing it is a deliberate divergence needing its own ADR.
- **86 top-level names skip the `n_`/`N` prefix.** 0 collisions measured against all 6,478
  top-level names in `lib/`, so this is forward risk only —
  [ADR 0005](../adr/0005-namespace-prefix-scope.md), and
  [architecture 002](../architecture/002-one-flat-namespace.md) for why it matters at all.

Deliberate divergences from the Rust oracle, each with an ADR: the loop-crossfade seam
([0001](../adr/0001-loop-crossfade-seam.md)), NaN clamping to a bound
([0002](../adr/0002-nan-clamps-to-the-bound.md)), and zone `volume` being applied at all
([0004](../adr/0004-apply-zone-volume-db.md)).

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
