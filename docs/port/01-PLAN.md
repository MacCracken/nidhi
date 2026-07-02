# nidhi Rust → Cyrius Port Plan

**Goal:** port nidhi (Rust sampler, 7180 lines, preserved in `rust-old/`) to Cyrius
**6.3.33**, reaching **behavioral parity** so the two can be benchmarked against each other.

`rust-old/` is the **parity oracle** — never modify it; cross-check every ported module against it.

## Key decisions (locked)

- **D1 — Samples/floats are f64.** Cyrius has no f32 width (everything is i64; floats are
  f64 bit-patterns). naad stores samples as "vec of f64"; shravan: "all samples are f64 bit
  patterns in vecs." So every Rust `f32` → f64. The port will NOT be bit-identical to Rust's
  f32 internals; that's expected and fine for benchmarking parity.
- **D2 — Errors are negative integer codes** (`ERR_* = (0 - N)`), matching the naad/hisab/
  shravan convention (`n_is_err`). The Rust `NidhiError` String payloads are dropped.
  Config/data types satisfy the "Serialize+Deserialize roundtrip" requirement natively via
  **`#derive(Serialize)`** (compiler-generated `X_to_json`/`X_from_json_str`, uses `lib/bayan.cyr`).
- **D3 — Symbol prefix `n_` / `N`** on every nidhi symbol (`NSample`, `NZone`, `n_flush_denormal`)
  to avoid collisions with naad/shravan/hisab in the flat concatenated bundle namespace.
- **D4 — Use naad FULLY** (user, 2026-07-02). Prefer naad's implementations over reproducing
  nidhi's own duplicated DSP — this also *cleans up* the port (no_std fallbacks, feature gates,
  duplicated DSP collapse). `[deps.naad] 2.1.0`, `[deps.shravan] 2.5.12`, `[deps.hisab] 2.6.7`
  (git+tag+`modules=["dist/X.cyr"]`) are declared and **resolved** (`cyrius deps` pulled 6 deps
  incl. transitive goonj/sakshi/sankoch; `cyrius.lock` written). naad DSP available: SVF/biquad
  filters, `envelope_adsr_*`, LFOs, effects, `hermite_interpolate`, `crossfade_equal_power`,
  `detect_onsets`, `normalize`/`rms`/`peak`, `db_to_amplitude`, `flush_denormal`, VoiceManager.
- **D7 — Parity = feature-set, not behavioral clone** (user, 2026-07-02). Reproduce the same
  capabilities (so the same benchmark workloads run), but take cleaner/better routes where naad
  or Cyrius idioms offer them. Behavior tracks naad (== Rust's std path).
- **D5 — No feature flags.** Cyrius `[features]` has no ecosystem adoption. `std`/`io`/`logging`/
  `simd`/`full` collapse into one full-featured build. Where Rust's std vs no_std numeric paths
  differ (e.g. `VelocityCurve::Convex` Babylonian sqrt), port the **no_std deterministic path**.
- **D6 — VERSION reset to 0.1.0**, grows toward 1.1.0 as parity is reached.

## Library layout (per hisab template)

```
cyrius.cyml         [package]/[build entry=programs/smoke.cyr]/[lib modules]/[deps]
VERSION             single line; manifest reads it via ${file:VERSION}
programs/smoke.cyr  [build].entry — proves the include chain links; NOT in the bundle
src/*.cyr           one module per Rust module; self-contained (NO cross-module include;
                    stdlib resolved by the consumer via [deps].stdlib)
dist/nidhi.cyr      bundle = [lib].modules concatenated in order (built by `cyrius distlib`)
tests/*.tcyr        assert suites (cyrius test / cyrius tests)
tests/*.bcyr        benches (cyrius bench)   tests/*.fcyr  fuzz (cyrius fuzz)
rust-old/           Rust parity oracle (read-only)
docs/port/          this plan + the 13 recon briefs (10-26)
```

## Module port order (dependency-ordered) & status

| # | src module | from Rust | deps | status |
|---|-----------|-----------|------|--------|
| 1 | `error.cyr` | error.rs | — | ✅ done (foundation.tcyr 14/0) |
| 2 | `f64_util.cyr` | lib.rs helper | math | ✅ done (`n_flush_denormal`, N_F32_MIN_POS/EPSILON) |
| 3 | `loop_mode.cyr` | loop_mode.rs | — | ✅ done |
| 4 | `envelope.cyr` | envelope.rs | **naad** | ✅ done (envelope.tcyr 19/0). naad-first: no_std fallback/EnvState/tick_no_std deleted; = NAdsrConfig + naad-Adsr builder + thin wrappers |
| 5 | `zone.cyr` | zone.rs | env, loop_mode | **NEXT**: Zone(32 fields)+VelocityCurve+FilterMode; matches()/playback_ratio() parity tests (brief 20) |
| 6 | `sample.cyr` | sample.rs | vec, math | NSample/NSampleBank/NSampleId; onset/slice |
| 7 | `instrument.cyr` | instrument.rs | zone | zone collection, round-robin |
| 8 | `stretch.cyr` | stretch.rs | vec, math | WSOLA/OLA; hot loops `#inline` |
| 9 | `effect_chain.cyr` | effect_chain.rs | **naad** | 5-slot chain over naad effects |
| 10 | `capture.cyr` | capture.rs | vec, math | record/trim/normalize/loop-detect |
| 11 | `sfz.cyr` | sfz.rs | str, hashmap | text parser (40+ opcodes); fuzz |
| 12 | `sf2.cyr` | sf2.rs | vec | RIFF binary parser; fuzz |
| 13 | `io.cyr` | io.rs | **shravan** | WAV load/stream |
| 14 | `engine.cyr` | engine.rs | **naad**, all above | core engine, voice mgmt, render loop — LAST |

## Parity test/bench/fuzz suites to build (mirror hisab's split)

- `tests/foundation.tcyr` (core — ✅ started), `tests/modules.tcyr` (per-module),
  `tests/edge_cases.tcyr` (degenerate loops, empty banks, zero-length samples),
  `tests/nidhi.tcyr` (primary).
- `tests/nidhi.bcyr` — reproduce the 7 Rust Criterion benchmarks (voice-count scaling, block vs
  per-sample fill, cubic/stereo interpolation, filtered render, WSOLA throughput) for Rust-vs-Cyrius
  comparison → `bench-history.csv`. (See brief 26 for the exact benchmark list.)
- `tests/nidhi.fcyr` — SFZ + SF2 parser fuzz (highest-value hostile-input surfaces).

## Cyrius gotchas (learned while porting — save time on every module)

- **No inline comments inside a `struct { }` body** — `field;  # note` breaks the parser
  (`expected '(', got fn` reported at a *later* line). Put field notes in a comment above the struct.
- **Self-contained modules + LSP**: `src/*.cyr` have NO includes; they rely on the test/bundle to
  include stdlib + deps first. The LSP analyzes a module standalone, so it flags cross-module
  symbols (e.g. `N_F32_EPSILON` from f64_util) as "undefined" — **expected**, not a real error;
  `cyrius test` (with the include chain) is the source of truth.
- **`#must_use`** on a fn whose result is legitimately discarded in a loop (e.g. an envelope
  `tick` advance) warns — drop `#must_use` there (mirrors that Rust's `tick()` isn't `#[must_use]`).
- **Consumer include order in tests**: `include "lib/hisab.cyr"` → `lib/goonj.cyr` → `lib/naad.cyr`
  → `src/*.cyr` (deps before the modules under test; stdlib auto-resolves from `[deps].stdlib`).
- **f64 literals**: build via `f64_from(int)` and `f64_div(f64_from(a), f64_from(b))` (e.g. 0.7 =
  `f64_div(f64_from(7), f64_from(10))`); `f64_to` truncates toward zero (matches Rust `as u32`).
- **`elif`** exists; `while` exists; use a sentinel assignment to break loops (avoid relying on `break`).

## Open question for the user

- **naad reuse vs reimplement** for functions nidhi's Rust *duplicated* rather than imported
  (its own `detect_onsets`, interpolation, normalize). Default: reuse naad where the Rust called
  naad; reimplement where the Rust had its own version (to keep parity with what Rust actually did).
