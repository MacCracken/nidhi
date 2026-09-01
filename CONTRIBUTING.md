# Contributing to nidhi

Thank you for your interest in contributing to nidhi.

nidhi is a **Cyrius** library — the sample playback engine for AGNOS. It began as a port of a
Rust crate; that heritage shows in the `Ports rust-old/src/X.rs` comments at the top of each
module, but the Rust source itself was retired in 2.0.4 and lives only in git history. There is
no `cargo` in this workflow.

## Development Workflow

1. Fork and clone the repository
2. Create a feature branch from `main`
3. Make your changes
4. Run the cleanliness check (below)
5. Open a pull request

## Prerequisites

- The Cyrius toolchain. **The version in `cyrius.cyml [package].cyrius` is the single source of
  truth** — never hardcode it anywhere else; CI reads the pin rather than duplicating it.

```bash
curl -sSf https://raw.githubusercontent.com/MacCracken/cyrius/main/scripts/install.sh | \
  CYRIUS_VERSION="$(grep '^cyrius = ' cyrius.cyml | sed 's/cyrius = "\(.*\)"/\1/')" sh
```

- `cyrius deps` then resolves the stdlib plus naad / shravan / hisab into `lib/`.

## Cleanliness Check

Every change must pass:

```bash
cyrius deps                                        # resolve lib/ from the pins
cyrius build programs/smoke.cyr build/nidhi-smoke   # the build-chain smoke test
cyrius test                                        # all tests/*.tcyr
cyrius fuzz                                        # fuzz/*.fcyr never-crash harnesses
cyrius distlib --check                             # dist/nidhi.cyr agrees with src/
```

`cyrius audit` runs fmt / lint / docs / tests / bench in one pass. It currently reports
pre-existing fmt, lint and undocumented-function findings; do not let your change add to them.

**A green suite is necessary, not sufficient.** Several defects fixed across 2.0.2–2.0.4 passed
every test at the time — a render path that leaked 2 MB/s, integer-PCM WAVs decoding to silence,
a loop crossfade that never closed its seam. If you are touching audio behaviour, add the
assertion that would have caught you.

## Code Conventions

Cyrius is everything-is-i64: no generics, no traits, no `bool`, no `f32`, no `Result`/`Option`.

- **Samples and floats are f64 bit patterns** carried in i64 — `f64_add`/`f64_mul`/…,
  `f64_from(int)` to build one, `f64_to(bits)` to extract (it **truncates toward zero**). A raw
  `0` where an f64 is expected is `+0.0`; those are the same bit pattern, which is what made the
  integer-PCM silence bug invisible for two releases.
- **Symbols are `n_`/`N`-prefixed** (`NSample`, `n_zone_new`). `dist/nidhi.cyr` is *concatenated*
  with naad / goonj / shravan / sankoch / hisab into one flat namespace, and Cyrius warns on a
  duplicate `fn` but is **silent on a duplicate `var`**. Error constants carry the full
  `NIDHI_ERR_` prefix for the same reason.
- **Errors are negative integers**; success is `0` or a positive pointer. Return
  `NIDHI_ERR_*` from `src/error.cyr`, never a bare negative literal.
- **`#must_use` on every fallible function.** If a call site genuinely does not need the result,
  wrap it in the test-local `ignore(...)` helper rather than leaving a warning.
- **No inline comments inside a `struct { }` body** — it breaks the parser. Put them above.
- **Check every `alloc()`** — it returns `0` on failure, and the allocator's free is a no-op, so
  an unchecked result becomes a null store.
- **Nothing allocates on the render path.** `tests/engine.tcyr` asserts an `alloc_used()` delta
  of exactly 0 across a block at 8 and 64 voices; keep it that way.
- **Use naad fully** for DSP — do not reimplement filters, envelopes, LFOs or effects it already
  provides, and prefer its allocation-free `*_into` / `*_core` forms on hot paths.
- Playback accuracy over speed; sample-accurate loop points and crossfades.

## Adding a New Module

1. Create `src/my_module.cyr`. Modules are **self-contained** — no `include` between `src/*.cyr`
   files; the bundle resolves them by concatenation order.
2. Add it to `[lib].modules` in `cyrius.cyml`, **in dependency order** — a module may only
   reference symbols defined above it.
3. Add `tests/my_module.tcyr`, including `src/error.cyr` and `src/f64_util.cyr` first (the
   module-order convention), then your module and its dependencies.
4. Run `cyrius distlib` so `dist/nidhi.cyr` picks it up.

## Parity questions

The correctness bar is still "matches what the Rust original did". To check one, see the ordered
procedure in [`CLAUDE.md`](CLAUDE.md) — golden vectors (`tests/golden.tcyr`) first, then the Rust
source from git history, then
[`docs/port/oracle-build-identity.md`](docs/port/oracle-build-identity.md) for the exact
dependency versions it was built against. **The original linked naad 1.2.5**, a different major
version from today's.

## Performance claims

Never claim a win without before/after numbers appended to `bench-history.csv`. Several
"obvious" optimisations have been measured and rejected for making things *worse*.

## License

By contributing, you agree that your contributions will be licensed under GPL-3.0-only.
