# The oracle's build identity

`rust-old/` — the Rust 1.1.0 crate the Cyrius port was written against — was removed from the
working tree in **2.0.4**. It remains in git history: `git log --all -- rust-old/` finds it, and
`git show <rev>:rust-old/src/engine.rs` retrieves any file verbatim.

Two things were **not** in git and would have been lost, so they are recorded here.

## Toolchain

```
rustc 1.97.1 (8bab26f4f 2026-07-14)
LLVM 22.1.6
x86_64-unknown-linux-gnu, target-feature sse2
edition 2024, rust-version 1.89 (Cargo.toml)
```

`rust-old/rust-toolchain.toml` pinned nothing, so the crate built against whatever rustc was
installed. The above is what the last recorded build used
(`rust-old/target/.rustc_info.json`).

## Dependency versions — what "parity" was measured against

`Cargo.lock` was untracked (`.gitignore:2` excludes `Cargo.lock`), which made it the single
unrecoverable file in the whole directory. Restored here in full.

**This matters** because the port's DSP behaviour tracks naad, and the oracle linked **naad
1.2.5** — a different major version from the naad 2.2.2 the Cyrius port links today. Any future
question of the form "did the original really do X?" has to be answered against 1.2.5, not
against current naad.

### Direct dependencies

| Crate | Version |
|---|---|
| `criterion` | 0.5.1 |
| `hisab` | 1.4.0 |
| `naad` | 1.2.5 |
| `serde` | 1.0.228 |
| `serde_json` | 1.0.150 |
| `thiserror` | 2.0.18 |
| `tracing` | 0.1.44 |

### Full transitive set

| Crate | Version |
|---|---|
| `aho-corasick` | 1.1.4 |
| `anes` | 0.1.6 |
| `anstyle` | 1.0.14 |
| `autocfg` | 1.5.1 |
| `bumpalo` | 3.20.3 |
| `cast` | 0.3.0 |
| `cfg-if` | 1.0.4 |
| `ciborium` | 0.2.2 |
| `ciborium-io` | 0.2.2 |
| `ciborium-ll` | 0.2.2 |
| `clap` | 4.6.1 |
| `clap_builder` | 4.6.0 |
| `clap_lex` | 1.1.0 |
| `criterion-plot` | 0.5.0 |
| `crossbeam-deque` | 0.8.6 |
| `crossbeam-epoch` | 0.9.18 |
| `crossbeam-utils` | 0.8.21 |
| `crunchy` | 0.2.4 |
| `either` | 1.16.0 |
| `futures-core` | 0.3.32 |
| `futures-task` | 0.3.32 |
| `futures-util` | 0.3.32 |
| `glam` | 0.29.3 |
| `half` | 2.7.1 |
| `hermit-abi` | 0.5.2 |
| `is-terminal` | 0.4.17 |
| `itertools` | 0.10.5 |
| `itoa` | 1.0.18 |
| `js-sys` | 0.3.103 |
| `lazy_static` | 1.5.0 |
| `libc` | 0.2.186 |
| `libm` | 0.2.16 |
| `log` | 0.4.33 |
| `matchers` | 0.2.0 |
| `memchr` | 2.8.2 |
| `nidhi` | 1.1.0 |
| `nu-ansi-term` | 0.50.3 |
| `num-traits` | 0.2.19 |
| `once_cell` | 1.21.4 |
| `oorandom` | 11.1.5 |
| `pin-project-lite` | 0.2.17 |
| `plotters` | 0.3.7 |
| `plotters-backend` | 0.3.7 |
| `plotters-svg` | 0.3.7 |
| `proc-macro2` | 1.0.106 |
| `quote` | 1.0.46 |
| `rayon` | 1.12.0 |
| `rayon-core` | 1.13.0 |
| `regex` | 1.12.4 |
| `regex-automata` | 0.4.14 |
| `regex-syntax` | 0.8.11 |
| `rustversion` | 1.0.22 |
| `same-file` | 1.0.6 |
| `serde_core` | 1.0.228 |
| `serde_derive` | 1.0.228 |
| `sharded-slab` | 0.1.7 |
| `shravan` | 1.1.0 |
| `slab` | 0.4.12 |
| `smallvec` | 1.15.2 |
| `syn` | 2.0.118 |
| `thiserror-impl` | 2.0.18 |
| `thread_local` | 1.1.9 |
| `tinytemplate` | 1.2.1 |
| `tracing-attributes` | 0.1.31 |
| `tracing-core` | 0.1.36 |
| `tracing-log` | 0.2.0 |
| `tracing-subscriber` | 0.3.23 |
| `unicode-ident` | 1.0.24 |
| `valuable` | 0.1.1 |
| `walkdir` | 2.5.0 |
| `wasm-bindgen` | 0.2.126 |
| `wasm-bindgen-macro` | 0.2.126 |
| `wasm-bindgen-macro-support` | 0.2.126 |
| `wasm-bindgen-shared` | 0.2.126 |
| `web-sys` | 0.3.103 |
| `winapi-util` | 0.1.11 |
| `windows-link` | 0.2.1 |
| `windows-sys` | 0.61.2 |
| `zerocopy` | 0.8.52 |
| `zerocopy-derive` | 0.8.52 |
| `zmij` | 1.0.21 |

## What replaced the oracle

Golden vectors captured from a live build before deletion, asserted by
[`tests/golden.tcyr`](../../tests/golden.tcyr) against
[`tests/golden-oracle.txt`](../../tests/golden-oracle.txt): `stretch_short` values at four
ratios, WSOLA and OLA interior values, the six `.round()` tie cases, and the
`detect_loop_points` candidate ordering. 43 assertions, all passing at the time of deletion.

To regenerate them, restore the crate from git and build a throwaway crate that depends on it by
path — do not modify the restored tree:

```sh
git show <rev>:rust-old/Cargo.lock > /tmp/oracle/Cargo.lock   # or use the table above
cargo run --offline --release                                  # in the throwaway generator
```

## Known inherited defect, recorded before deletion

`TimeStretcher::stretch` amplifies its output by up to **~588x** (measured: input max 0.97 →
output max 588.5). `normalize_by_window_sum` divides by `window_sum` only where it exceeds
`1e-6`, so near the output edges — where a single Hann frame overlaps and its taper is tiny —
the division multiplies instead of normalising. The Cyrius port reproduces this faithfully
(identical `1e-6` threshold and `>` guard at `src/stretch.cyr`), so it is an **inherited
upstream defect, not a port defect**. Fixing it would be a deliberate divergence and needs its
own ADR; the golden vectors above deliberately sample the *interior* region so they measure the
algorithm rather than this amplification.
