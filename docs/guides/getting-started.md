# Getting started with nidhi

## Build

```sh
cyrius deps                                        # resolve stdlib + naad/shravan/hisab into lib/
cyrius build programs/smoke.cyr build/nidhi-smoke   # the [build].entry -- a build-chain smoke test
cyrius test                                        # every tests/*.tcyr
cyrius fuzz                                        # fuzz/*.fcyr never-crash harnesses
cyrius distlib                                     # rebuild dist/nidhi.cyr, the consumer bundle
```

nidhi is a **library**. There is no `src/main.cyr`; `[build].entry` is `programs/smoke.cyr`, which
exists only to prove the build chain links. The product is `dist/nidhi.cyr`.

## Layout

- `src/*.cyr` — the 14 library modules, listed in dependency order in `cyrius.cyml [lib].modules`.
  They are **self-contained**: no `include` between them. The bundle is a plain concatenation, so
  a module may reference symbols from any module listed above it and nothing below.
- `tests/*.tcyr` — one suite per module, auto-discovered by `cyrius test`.
- `tests/golden-oracle.txt` — values captured from the original Rust crate before it was retired
  in 2.0.4, asserted by `tests/golden.tcyr`. The Rust source itself is in git history
  (`git show <rev>:rust-old/src/...`), not the working tree.
- `fuzz/*.fcyr` — parser harnesses; they must return an error code, never crash, on any input.
- `dist/nidhi.cyr` — the bundle consumers take. Rebuild it whenever `src/` changes.

## Adding a module

1. Create `src/my_module.cyr`. No `include` of another `src/` file.
2. Add it to `[lib].modules` in `cyrius.cyml`, **in dependency order** — it may only use symbols
   from modules above it.
3. Create `tests/my_module.tcyr`, including `src/error.cyr` and `src/f64_util.cyr` first (module
   order), then your module and its dependencies.
4. `cyrius test`, then `cyrius distlib`.

## Changing behaviour

The correctness bar is "matches what the Rust original did". To check something:

1. `tests/golden.tcyr` — values captured from a live oracle build.
2. `git show <rev>:rust-old/src/<mod>.rs` — the full Rust source, in history forever.
   ⚠ Under zsh, brace the variable: `${REV}:rust-old/...`, or `$REV:r` is parsed as a modifier.
3. [`../port/oracle-build-identity.md`](../port/oracle-build-identity.md) — the exact dependency
   versions it was built against. **The oracle linked naad 1.2.5**, a different major version
   from today's.

**A green suite is necessary, not sufficient.** Several defects fixed across 2.0.2–2.1.0 passed
every test at the time — a render path leaking 2 MB/s, integer-PCM WAVs decoding to silence, a
loop crossfade that never closed its seam, a zone `volume` that was parsed and ignored. If you
are touching audio behaviour, add the assertion that would have caught you, and consider a render
differential: dump raw bit patterns across configurations before and after, and `cmp`. That is
how every 2.0.7 optimisation was proven bit-identical.

## Releasing

1. `./bump-version.sh X.Y.Z` (VERSION is the single source of truth; `cyrius.cyml` derives it).
2. Add a CHANGELOG entry.
3. `cyrius distlib` to restamp the bundle.
4. Record any performance change in `bench-history.csv` — never claim a win without before/after.

See [`../adr/template.md`](../adr/template.md) when a choice deserves an ADR — in particular any
**deliberate divergence from the oracle**. There are five so far.
