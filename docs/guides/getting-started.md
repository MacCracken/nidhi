# Getting started with nidhi

## Build

```sh
cyrius deps                              # resolve dependencies
cyrius build src/main.cyr build/nidhi    # compile
cyrius test                              # run tests/*.tcyr
```

## Layout

- `src/main.cyr` — entry point. Top-level `var r = main(); syscall(SYS_EXIT, r);`.
- `tests/` — test suite (`.tcyr` files, auto-discovered by `cyrius test`).
- `tests/golden-oracle.txt` — values captured from the original Rust crate before it was
  retired in 2.0.4; asserted by `tests/golden.tcyr`. The Rust source itself lives in git
  history (`git show <rev>:rust-old/src/...`), not in the working tree.

## Adding a feature

1. Edit `src/main.cyr` (or add a new module and `include` it).
2. Cross-check parity against `tests/golden.tcyr` and, for anything it does not cover, the
   Rust source in git history. See CLAUDE.md "Port conventions".
3. Add a test case to `tests/nidhi.tcyr`.
4. Run `cyrius test`.
5. Bump `VERSION` and add a CHANGELOG entry before tagging.

See [`../adr/template.md`](../adr/template.md) when a non-trivial design choice deserves an ADR.
