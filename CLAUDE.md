# nidhi — Claude Code Instructions

> **Core rule**: this file is **preferences, process, and procedures** — durable rules that
> change rarely. Volatile state (current version, module line counts, port progress, test
> counts) lives in `VERSION`, `CHANGELOG.md`, and `docs/port/01-PLAN.md`. Do not inline state here.

## Project Identity

**nidhi** (Sanskrit: *treasure*) — the **sample playback engine** for AGNOS. Cyrius port of the
Rust 1.1.0 crate (7180 lines preserved at `rust-old/` as the parity oracle).

- **Type**: Port (Rust → Cyrius), flat library
- **License**: GPL-3.0-only
- **Language**: Cyrius (toolchain pinned in `cyrius.cyml [package].cyrius`)
- **Version**: `VERSION` at the project root is the source of truth — do not inline the number here
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-standards.md) · [First-Party Documentation](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-documentation.md)

## Goal

nidhi OWNS sample playback in the AGNOS audio stack: key/velocity zones, loop modes, crossfades,
per-zone filters/envelopes/LFOs, time-stretching, and SFZ/SF2 import — a polyphonic sampler engine
consumed by **dhvani** (audio engine) and thereby **shruti** (DAW), replacing
`shruti-instruments::sampler`. It leans on **naad** for DSP so it stays a thin, correct sampler.

## Dependencies

Declared in `cyrius.cyml [deps.*]` (git + tag + `dist/*.cyr` bundle), resolved by `cyrius deps`:

- **naad** (2.1.0) — DSP: SVF/biquad filters, ADSR, LFOs, effects, voice management, interpolation,
  onset/normalize helpers. **Use naad fully** — do not reimplement DSP naad already provides.
- **shravan** (2.5.12) — audio codecs (WAV decode/encode + streaming), behind the `io` module.
- **hisab** (2.6.7) — math (pulled transitively by naad; also directly available).

## Quick Start

```sh
cyrius deps                                     # resolve stdlib + naad/shravan/hisab bundles
cyrius build programs/smoke.cyr build/nidhi-smoke   # build the [build].entry (smoke, proves the chain links)
cyrius test                                     # run every tests/*.tcyr
cyrius tests tests                              # same, explicit dir
cyrius bench tests/nidhi.bcyr                   # run the parity benchmarks
cyrius fuzz                                     # run fuzz/*.fcyr never-crash harnesses
cyrius distlib                                  # rebuild dist/nidhi.cyr (the consumer bundle)
```

nidhi is a **library** (hisab-style layout), not a binary: `[build].entry` is
`programs/smoke.cyr` (a minimal build-chain smoke test); the real code is the `[lib].modules`
list, bundled into `dist/nidhi.cyr` for consumers via `cyrius distlib`.

## Module structure (`src/*.cyr`, dependency order)

`error` → `f64_util` → `loop_mode` → `envelope` (naad ADSR) → `zone` → `sample` → `instrument`
→ `capture` → `stretch` → `effect_chain` (naad effects) → `io` (shravan WAV) → `sf2` (RIFF parser)
→ `sfz` (text parser) → `engine` (voice mgmt + render loop; depends on all above + naad).

Modules are **self-contained**: NO `include` between `src/*.cyr` files; stdlib auto-resolves from
`[deps].stdlib`; first-party deps are `include "lib/hisab.cyr"` / `lib/goonj.cyr` / `lib/naad.cyr`
/ `lib/shravan.cyr` in the tests/bench/fuzz files (dependency order). The bundle concatenates
`[lib].modules` in order.

## Port conventions (durable)

- **Cross-check against `rust-old/`** — correctness bar is "matches what Rust did". The
  adversarial parity audit lives at `scratchpad/parity_audit.wf.js` (a Workflow); re-run it after
  changes to hunt divergences tolerance tests miss.
- **Parity = feature-set, not behavioral clone** — reproduce capabilities (same benchmarks), but
  take cleaner routes where naad/Cyrius idioms offer them. Behavior tracks naad (== Rust std path).
- **Samples/floats are f64** — Cyrius has no f32; every Rust `f32` is an f64 bit-pattern via
  `f64_add/mul/div/...`, `f64_from(int)` / `f64_to(bits)` (truncates). Matches naad/shravan.
- **Symbols are `n_`/`N`-prefixed** (`NSample`, `n_zone_new`) to avoid collisions in the flat
  concatenated bundle namespace.
- **Errors are negative integer codes** (naad/hisab convention); config types use
  `#derive(Serialize)` (bayan JSON) for the serde-roundtrip requirement.
- **No inline comments inside a `struct { }` body** (breaks the Cyrius parser — put them above).
- Playback accuracy over speed; sample-accurate loop points and crossfades.

## Testing / benchmarking

- Tests: `tests/*.tcyr` (assert / assert_eq / test_group / assert_summary). Every module has a
  suite porting the Rust `#[cfg(test)]` cases as parity checks.
- Benchmarks: `tests/nidhi.bcyr` reproduces the 7 Rust criterion benchmarks; results in
  `BENCHMARKS.md` + `docs/benchmarks-rust-v-cyrius.md`, series in `bench-history.csv`.
- Fuzz: `fuzz/fuzz_sf2.fcyr` + `fuzz/fuzz_sfz.fcyr` — parsers must return an error code, never
  crash, on any input.
- Never claim a perf win without before/after numbers in `bench-history.csv`.

## Rules (Hard Constraints)

- **Do not commit or push** — the user handles all git operations
- **Never use `gh` CLI** — use `curl` to the GitHub API if needed
- **Do not modify `rust-old/`** — it's the parity oracle
- Do not skip tests before claiming changes work
- Do not modify `lib/` files (vendored stdlib / dep bundles, regenerated by `cyrius deps`)
- Do not hardcode toolchain versions in CI YAML — `cyrius = "X.Y.Z"` in `cyrius.cyml` is the source of truth
- Do not add unnecessary dependencies

## Documentation

- [`docs/port/01-PLAN.md`](docs/port/01-PLAN.md) — the port plan, locked decisions, status, and Cyrius gotchas
- [`docs/port/`](docs/port/) — per-module port specs + language/stdlib/dep briefs (10–26)
- [`BENCHMARKS.md`](BENCHMARKS.md) · [`docs/benchmarks-rust-v-cyrius.md`](docs/benchmarks-rust-v-cyrius.md) — perf
- [`docs/adr/`](docs/adr/) — Architecture Decision Records (*why X over Y?*)
