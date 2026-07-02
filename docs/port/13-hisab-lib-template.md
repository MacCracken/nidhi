# 13 — hisab as the Cyrius Library Template for nidhi

Read-only reconnaissance of `/home/macro/Repos/hisab` (a shipped Cyrius **library**
port of a Rust math crate) to derive the exact recipe nidhi's Rust→Cyrius port must
follow. hisab is the closest structural analog: flat library crate, many pure-data
modules, heavy f64 math, integer error codes.

> **Cyrius model recap** (holds throughout): everything is `i64`. Structs are heap
> blobs of untyped 8-byte slots. `#derive(accessors)` on `struct T { a; b; }` generates
> `T_a(p)` / `T_set_a(p, v)` (getter reads `load64(p+0)`, setter writes `store64(p+0)`).
> Floats are IEEE-754 **bit patterns** in an i64, manipulated via `f64_add/f64_sub/
> f64_mul/f64_div/f64_lt/f64_sqrt/...` and converted with `f64_from(int)` / `f64_to(bits)`.
> Errors are **negative i64 codes**; success is `0` or a valid pointer. No serde, no
> generics, no traits, no closures (use function pointers via `fnptr` + `fncall1/2`).

---

## 1. Repository layout (the template)

```
hisab/
  cyrius.cyml         package manifest — [package] [build] [lib] [deps]
  cyrius.lock         SHA256 lockfile (written by `cyrius deps`)
  VERSION             single line, e.g. "2.6.7"; manifest pulls it via ${file:VERSION}
  src/
    main.cyr          CLI smoke binary — prints version, exits. Does NOT include the library.
    error.cyr         integer error-code module (FIRST in dependency order)
    f64_util.cyr      f64 helpers not in stdlib
    <N library modules>.cyr   one module per file, self-contained (NO include lines)
  lib/                VENDORED stdlib + first-party deps ONLY. Managed by `cyrius deps`.
                      Never put project source here. (alloc.cyr, math.cyr, vec.cyr,
                      assert.cyr, bench.cyr, fnptr.cyr, ganita.cyr, io.cyr, sakshi.cyr, ...)
  dist/
    hisab.cyr         single-file bundle of all [lib] modules, concatenated in
                      dependency order. Consumed by downstream projects.
  tests/
    *.tcyr            assertion suites (run: `cyrius test <f>` / `cyrius tests`)
    *.bcyr            benchmark harness   (run: `cyrius bench <f>`)
    *.fcyr            fuzz harness        (built as a normal binary, run under timeout)
  build/              compiled binaries — gitignored, regenerated
  examples/*.cyr      small demos
  scripts/            bench-history.sh, version-bump.sh
  docs/               architecture/, development/, guides/, audit/
  .github/workflows/  ci.yml, release.yml
```

**hisab has NO Makefile and NO top-level `benches/` dir.** Benches live in `tests/*.bcyr`.
(nidhi's Rust repo has a `Makefile` and `benches/` — those disappear in the Cyrius port;
the Cyrius equivalent of `make check` is `cyrius audit`.)

---

## 2. `cyrius.cyml` manifest — the exact shape

Full hisab manifest (adapt names/modules for nidhi):

```toml
[package]
name = "hisab"
version = "${file:VERSION}"          # pulled from the VERSION file — do NOT hardcode
description = "hisab — higher mathematics library ..."
license = "GPL-3.0-only"
repository = "https://github.com/MacCracken/hisab"
language = "cyrius"
cyrius = "6.3.11"                     # toolchain pin; CI/release grep this, not YAML

[build]
src = "src/main.cyr"                  # the CLI smoke entry
output = "build/hisab"

[lib]                                 # declares the dist bundle layout, dependency-ordered
modules = [
    "src/error.cyr",
    "src/f64_util.cyr",
    "src/vec2.cyr",
    ...                               # every library module, in dependency order
]

[deps]
stdlib = [                            # stdlib modules vendored into lib/ by `cyrius deps`
    "syscalls", "string", "alloc", "str", "fmt", "vec", "io", "args",
    "assert", "math", "ganita", "tagged", "fnptr", "bench", "callback",
]

[deps.sakshi]                         # first-party git dep, pinned by tag, pulled as a bundle
git = "https://github.com/MacCracken/sakshi.git"
tag = "2.4.2"
modules = ["dist/sakshi.cyr"]
```

Key rules encoded here:
- `version = "${file:VERSION}"` — the manifest auto-syncs from `VERSION`. Bump `VERSION`, not the manifest.
- `[lib].modules` is **dependency-ordered** (a module may reference symbols from any module listed *above* it, because the bundle is a plain concatenation — see §6).
- `[deps].stdlib` is the list of stdlib modules the **consumer** must have available. Library `src/*.cyr` files never `include` stdlib; they assume these symbols are present at compile time.
- Downstream projects consume the bundle by adding `[deps.nidhi] modules = ["dist/nidhi.cyr"]` to *their* manifest.

---

## 3. The self-contained-module rule (CRITICAL)

Every `src/*.cyr` **library** module is self-contained:

- **NO `include` lines inside `src/*.cyr` library modules.** They declare `struct`s and
  `fn`s directly. Stdlib symbols (`alloc`, `store64`, `f64_add`, `vec_push`, `F64_ONE`,
  `EPSILON_F64`, …) are assumed to be in scope, resolved by whoever compiles the module
  (a test file, a bench file, or the consumer's manifest via `[deps].stdlib`).
- Each module's **header comment documents its `Requires:`** — the stdlib/other modules
  that must be present. Example from `num.cyr`:
  ```
  # num.cyr -- numerical methods for hisab
  # Usage: include "lib/num.cyr"
  # Requires: alloc.cyr, vec.cyr, fnptr.cyr, math.cyr, f64_util.cyr, error.cyr
  ```
  This is documentation only — the module has no actual `include`.
- **Only two files use `include`:** (a) `src/main.cyr` (the CLI smoke binary, includes just
  the stdlib bits it needs from `lib/`), and (b) `tests/*.tcyr` / `*.bcyr` / `*.fcyr`, which
  `include "src/<module>.cyr"` to pull the modules under test.
- **Consequence for ordering:** because there are no includes, a module referencing another
  module's symbol just works *as long as the two are concatenated/included in the right order*.
  `error.cyr` and `f64_util.cyr` come first everywhere; everything else follows.

---

## 4. `src/main.cyr` — the minimal CLI smoke test

The entire hisab CLI (it does **not** include the library — library coverage is in tests):

```
# hisab — higher mathematics library (CLI entry)
include "lib/syscalls.cyr"
include "lib/io.cyr"

fn main() {
    println("hisab 2.6.7");
    return 0;
}

var r = main();
syscall(SYS_EXIT, r);
```

Rules visible here:
- Programs (not libraries) `include` stdlib from `lib/` explicitly.
- The program calls `main()` at top level and exits via syscall: `var r = main(); syscall(SYS_EXIT, r);`
  (`SYS_EXIT` comes from `syscalls.cyr`; hisab also writes `syscall(60, r)` in some docs — 60 is the raw x86-64 exit number).
- `main()` returns `0`. `[build]` compiles `src/main.cyr` → `build/hisab`.
- For nidhi: `src/main.cyr` should `println("nidhi 1.1.0");` and exit. Keep it minimal — its only job is to prove the toolchain can build the package.

---

## 5. Module anatomy — struct + accessors + attributes + error codes

### 5a. `#derive(accessors)` structs (from `vec2.cyr`, `complex.cyr`)

```
#derive(accessors)
struct HVec2 { x; y; }              # untyped fields; each slot is 8 bytes

#must_use
fn hvec2_new(x, y) {                # x, y are f64 BIT PATTERNS
    var v = alloc(16);              # 2 fields * 8 bytes
    store64(v, x);                  # field 0 at offset 0
    store64(v + 8, y);             # field 1 at offset 8
    return v;                       # returns a pointer (i64)
}
```

`#derive(accessors)` on `HVec2` auto-generates:
- getter `HVec2_x(p)` → `load64(p + 0)`, `HVec2_y(p)` → `load64(p + 8)`
- setter `HVec2_set_x(p, v)` → `store64(p + 0, v)`, `HVec2_set_y(p, v)` → `store64(p + 8, v)`

Naming convention: `TypeName_field` (get) and `TypeName_set_field` (set). Types are
CamelCase with a project prefix (`HVec2`, `HComplex`, `HMat4`) to avoid symbol collisions
in the concatenated bundle. **Give nidhi a prefix — `N` (e.g. `NSample`, `NZone`, `NVoice`)
— to keep bundle symbols unique.**

Two idioms coexist and are both fine:
- **Accessor idiom** (`complex.cyr`): declare the struct with `#derive(accessors)`, use
  `HComplex_re(c)` / `HComplex_set_im(c, v)`.
- **Manual-offset idiom** (`complex.cyr`'s `ComplexMatrix`, `num.cyr`'s PCG state): no struct
  decl, just `alloc(N)` + hand-written `load64(p + OFF)` / `store64(p + OFF, v)` with named
  offset constants in comments (`# { rows: i64, cols: i64, data_ptr: i64 } = 24 bytes`).
  Used for variable-length / array-backed structures where accessors don't fit.

### 5b. `#must_use` and `#inline`

- `#must_use` sits on the line **directly above** a pure/constructor/accessor `fn` (mirrors
  Rust `#[must_use]`). hisab puts it on nearly every pure function: `#must_use fn cx_add(a,b){...}`.
- `#inline` is applied the same way (line above `fn`) for hot-path functions. (hisab's math is
  mostly `#must_use`; nidhi's per-sample render loop is where `#inline` matters — put it on the
  hot render/interpolation functions.)
- These are **line-above attributes**, one per line, stacking like Rust:
  ```
  #inline
  #must_use
  fn n_render_sample(...) { ... }
  ```

### 5c. Error codes (from `error.cyr`) — replaces the Rust `NidhiError` enum

```
var ERR_NONE              = 0;
var ERR_INVALID_TRANSFORM = (0 - 1);      # note: negative literals written as (0 - N)
var ERR_SINGULAR_MATRIX   = (0 - 2);
...
var EPSILON_F64 = 0x3D719799812DEA11;      # 1e-12 as f64 bits — a shared tolerance constant

fn hisab_is_err(code) {
    if (code < 0) { return 1; }
    return 0;
}
```

Conventions:
- `error.cyr` is module #1 in `[lib]` and included first in every test/bench/fuzz file.
- Functions return `0` (`ERR_NONE`) on success, a negative `ERR_*` on failure, and write
  real results through **out-param pointers** (`store64(out, value); return ERR_NONE;`).
  See `num_newton(f, df, x0, tol, max_iter, out)`.
- Or, for constructor-style functions, return a pointer (nonzero) on success and `0` on
  failure (see `cmat_new` returning `0` on bad dims).
- f64 tolerance constants (`EPSILON_F64`) live in `error.cyr` and are reused everywhere.
- **nidhi mapping:** the 5 `NidhiError` variants become integer codes, e.g.
  `ERR_SAMPLE_NOT_FOUND = (0 - 1)`, `ERR_INVALID_ZONE = (0 - 2)`,
  `ERR_INVALID_PARAMETER = (0 - 3)`, `ERR_PLAYBACK = (0 - 4)`, `ERR_IMPORT = (0 - 5)`.
  The string payloads in the Rust enum are dropped (no error strings; the code is the error).

### 5d. f64 helpers (from `f64_util.cyr`)

f64 values are bit patterns. Stdlib `math`/`ganita` provide most ops; the module supplies
the missing few. Constants are literal hex bit patterns:

```
var _NUM_F64_ONE  = 0x3FF0000000000000;   # 1.0
var _NUM_F64_TWO  = 0x4000000000000000;   # 2.0
var _NUM_F64_HALF = 0x3FE0000000000000;   # 0.5

fn f64_tan(x) { return f64_div(f64_sin(x), f64_cos(x)); }
fn f64_approx_eq(a, b, tol) {
    if (f64_lt(f64_abs(f64_sub(a, b)), tol) == 1) { return 1; }
    return 0;
}
```

Note: comparison ops (`f64_lt`, `f64_gt`) **return 1/0**, so idiom is `if (f64_lt(a,b) == 1)`.
`f64_from(n)` converts integer→f64 bits; `f64_to(bits)` converts f64 bits→integer (truncating).

### 5e. Function pointers & higher-order (replaces Rust closures)

Callbacks are stdlib `fnptr` symbols: pass a function name as a value, invoke with
`fncall1(f, x)` (1 arg) or `fncall2(f, a, b)` (2 args). Example from `num.cyr`:
```
fn num_newton(f, df, x0, tol, max_iter, out) {
    var fx = fncall1(f, x);
    ...
}
```
nidhi will need this for any place the Rust code took an `Fn`/callback (e.g. a per-sample
processing hook). No closures — a fnptr plus explicit context pointer.

---

## 6. The dist bundle — how `dist/nidhi.cyr` is produced

**What it is:** `dist/hisab.cyr` is a single file = every `[lib].modules` file concatenated
in order, each preceded by a `# --- modulename.cyr ---` banner. Header:

```
# hisab.cyr -- bundled distribution
# Version: 2.6.7
# Generated by: cyrius distlib
# Do not edit -- rebuild with: cyrius distlib

# --- error.cyr ---
<full text of src/error.cyr>
# --- f64_util.cyr ---
<full text of src/f64_util.cyr>
...
```

It is a **strip-include concatenator**: because `src/*.cyr` have no includes, concatenation
in dependency order yields a compilable single file. Consumers add
`[deps.nidhi] modules = ["dist/nidhi.cyr"]` and get the whole library as one bundle.

**Which subcommand builds it — IMPORTANT DISCREPANCY:**
- hisab's `cyrius.cyml`, `CLAUDE.md`, and both `.github/workflows/{ci,release}.yml` all say
  **`cyrius distlib`** (valid in the pinned toolchain, Cyrius 6.3.11).
- The toolchain actually installed on this machine is **Cyrius 6.3.32**, and its
  `cyrius --help` does **not** list `distlib`. In 6.3.x the relevant subcommands are:
  - `cyrius lib sync [--dry-run] [--full]` — vendor declared `[deps].stdlib` into `lib/`
    (this is the newer name for what hisab's docs call `cyrius deps` for stdlib).
  - `cyrius package` — create a `.ark` distributable.
  - `cyrius deps [--no-lock|--verify]` — resolve `[deps.NAME]` into `lib/`, write `cyrius.lock`.
- **Recommendation for nidhi:** keep the `[lib].modules` declaration exactly as hisab does
  and follow hisab's documented workflow (`cyrius distlib` to regenerate `dist/nidhi.cyr`).
  If `distlib` is absent on the installed toolchain, the bundle is trivially reproducible by
  concatenating `[lib].modules` in order with `# --- <name> ---` banners (that IS all distlib
  does). Verify the exact subcommand against whatever toolchain version nidhi pins in its own
  `cyrius.cyml` — do not assume; run `cyrius --help` first.

**Deps resolution:** `cyrius deps` reads `[deps]` and writes `lib/*.cyr` (stdlib snapshot +
first-party bundles like `sakshi.cyr`) and `cyrius.lock`. `cyrius deps --verify` checks the
committed lock in CI. `lib/` is git-ignored/regenerated conceptually but hisab commits it.

---

## 7. Tests — `tests/*.tcyr` format and how to run

A `.tcyr` file is an executable Cyrius program that `include`s the src modules it exercises,
calls `alloc_init();`, runs assertions, and ends with `var r = assert_summary();`.
Structure (from `foundation.tcyr` / `hisab.tcyr`):

```
# foundation.tcyr — tests for foundation types
include "src/f64_util.cyr"          # dependency order: error/f64_util first
include "src/error.cyr"
include "src/vec2.cyr"
include "src/vec3.cyr"
...

alloc_init();                       # MUST init the allocator before any alloc()

# optional local helpers / tolerances
var LOOSE_TOL = 0x3E45798EE2308C3A;  # ~1e-8 as f64 bits
fn assert_f64_eq(a, b, msg) {
    assert(f64_approx_eq(a, b, LOOSE_TOL), msg);
    return 0;
}

test_group("vec2 construction");    # section label, printed by the harness
var v = hvec2_new(f64_from(3), f64_from(4));
assert_eq(f64_to(HVec2_x(v)), 3, "v2 new.x");
assert_eq(f64_to(HVec2_y(v)), 4, "v2 new.y");
...

var r = assert_summary();           # prints pass/fail totals; nonzero exit on failure
```

**Assertion API** (stdlib `lib/assert.cyr`, auto-in-scope):
`assert(cond, name)`, `assert_eq(a,b,name)`, `assert_neq`, `assert_gt`, `assert_lt`,
`assert_gte`, `assert_lte`, `assert_nonnull(p,name)`, `assert_streq(a,b,name)`,
`assert_fatal(cond,msg)`, `panic(msg)`, `test_group(name)`, `assert_summary()`.
Assertions compare **integers**, so f64 checks go through `f64_to(...)` (exact) or
`f64_approx_eq` (tolerance).

**Running tests:**
- `cyrius test tests/nidhi.tcyr` — compile one suite, run it, require exit 0.
- `cyrius tests` — recursively run every `.tcyr` under `tests/` (default dir). CI loops:
  `for f in tests/*.tcyr; do cyrius test "$f"; done`.
- hisab ships 4 suites: `hisab.tcyr` (primary), `foundation.tcyr` (core types),
  `modules.tcyr` (per-module coverage), `edge_cases.tcyr` (degenerate/boundary inputs).
  957 assertions total. **Give nidhi the same split.**

**Fuzz `*.fcyr`:** same include-src pattern, defines `fuzz_*` targets that read arbitrary
bytes via `load64(data+off)` and check invariants (no crash, finite outputs). Built as a
normal binary and run under `timeout`: `cyrius build tests/x.fcyr build/x && timeout 10 build/x`.
Run all via `cyrius fuzz` (reads `fuzz/*.fcyr` per the CLI). hisab keeps its fuzz harness in
`tests/hisab.fcyr`; nidhi's Rust `fuzz/` dir maps here.

---

## 8. Benchmarks — `tests/*.bcyr` format and how to run

A `.bcyr` file includes src modules, defines zero-arg bench functions, and drives them with
the stdlib `bench` API. From `hisab.bcyr`:

```
include "src/f64_util.cyr"
include "src/error.cyr"
include "src/vec3.cyr"
...

fn bench(name, fp, n) {             # local convenience wrapper
    var b = bench_new(name);
    bench_run(b, fp, n);
    bench_report(b);
    return 0;
}

var _bv3a = 0;                      # pre-allocated test data, set up in main()
fn bench_vec3_add() { var r = hvec3_add(_bv3a, _bv3b); return 0; }   # a bench target

# batch pattern to clear the ~488ns timer floor:
fn bench_vec3_dot_x64() {
    var i = 0; var acc = 0;
    while (i < 64) { acc = acc + f64_to(hvec3_dot(_bv3a, _bv3b)); i = i + 1; }
    return acc;
}
```

**Bench API** (`lib/bench.cyr`): `bench_new(name)`, `bench_run(b, fnptr, n)`,
`bench_run_batch(b, fp, batch_size, rounds)`, `bench_run_batch1/2(...)` (pass args),
`bench_report(b)`, `bench_avg_ns/min_ns/max_ns(b)`, `now_ns()`. Single ops below the
~488ns timer floor must be amplified with an inner loop (the `_x64` idiom).

**Running:** `cyrius bench tests/nidhi.bcyr`. CI loops `for f in tests/*.bcyr; do cyrius bench "$f"; done`.
`scripts/bench-history.sh` appends a row to `bench-history.csv` (regression baseline).
Per CLAUDE.md: **never claim a perf win without before/after numbers in the CSV.**

---

## 9. CI / release gates (`.github/workflows`)

CI runs, in order: `cyrius fmt --check` per source · `cyrius vet src/main.cyr` ·
**dist-drift check** (`cyrius distlib` then `git diff --quiet dist/nidhi.cyr` — fails if stale) ·
`cyrius build src/main.cyr build/nidhi` + ELF magic check (`7f45 4c46`) ·
`cyrius test` over `tests/*.tcyr` · fuzz over `tests/*.fcyr` under `timeout 10` ·
`cyrius bench` over `tests/*.bcyr`. Release additionally asserts `VERSION == git tag`.
Toolchain version is grepped from `cyrius.cyml` (`cyrius = "..."`), never hardcoded in YAML.
`cyrius audit` is the local one-shot gate (fmt/lint/docs/tests/bench).

---

## 10. Concrete src/ module list for nidhi (mapped from Rust)

Rust `src/*.rs` → Cyrius `src/*.cyr`, listed in **`[lib]` dependency order**
(error/util first; leaf data types before the engine that consumes them):

| # | Cyrius module        | From Rust        | Notes / prefix                                                        |
|---|----------------------|------------------|----------------------------------------------------------------------|
| 1 | `src/error.cyr`      | `error.rs`       | `ERR_*` codes (5 variants → negatives), `EPSILON_F64`, `n_is_err`.    |
| 2 | `src/f64_util.cyr`   | (new / `lib.rs`) | f64 helpers incl. `flush_denormal` (subnormal→0), tolerance consts.   |
| 3 | `src/loop_mode.cyr`  | `loop_mode.rs`   | `LoopMode` enum → `var LOOP_ONESHOT=0; LOOP_FORWARD=1; ...` constants.|
| 4 | `src/sample.cyr`     | `sample.rs`      | `NSample`, `NSampleBank`, `NSampleId`; onset/slice; array-backed.     |
| 5 | `src/envelope.cyr`   | `envelope.rs`    | `NAmpEnvelope`, `NAdsrConfig` (no_std fallback path).                 |
| 6 | `src/loop_zone`→`zone.cyr` | `zone.rs`  | `NZone` (key/vel, filter, env, LFO, bus routing).                    |
| 7 | `src/stretch.cyr`    | `stretch.rs`     | WSOLA / OLA time-stretch; hot loops get `#inline`.                    |
| 8 | `src/instrument.cyr` | `instrument.rs`  | `NInstrument` (zone collection, round-robin).                        |
| 9 | `src/effect_chain.cyr`| `effect_chain.rs`| per-instrument effect chain (naad effects → local or dep).          |
|10 | `src/capture.cyr`    | `capture.rs`     | recording, trim, normalize, loop detection.                          |
|11 | `src/sfz.cyr`        | `sfz.rs`         | SFZ text parser — big; fuzz it.                                       |
|12 | `src/sf2.cyr`        | `sf2.rs`         | SF2/SoundFont binary parser — big; fuzz it.                          |
|13 | `src/io.cyr`         | `io.rs`          | WAV load/stream (Rust: behind `io` feature; Cyrius: separate module).|
|14 | `src/engine.cyr`     | `engine.rs`      | core engine, voice mgmt, render loop — LAST; depends on all above. Hot render fns `#inline`. |

Plus `src/main.cyr` (CLI smoke — prints `nidhi <VERSION>`, exits; not in `[lib]`).

**Feature gates** (`#[cfg(feature="io")]`, `std` vs `no_std`) do not exist in Cyrius — there
are no per-module feature flags in the library. Everything ships in the bundle; the `io`,
`std`, `naad` (effects), and `shravan` (codecs) gating collapses: either port those modules
inline (`io.cyr`, `effect_chain.cyr`) or pull them as first-party `[deps.NAME]` bundles the
way hisab pulls `sakshi`.

**Test/bench/fuzz files to create** (mirror hisab's split):
`tests/nidhi.tcyr` (primary), `tests/foundation.tcyr` (sample/zone/envelope core),
`tests/modules.tcyr` (per-module), `tests/edge_cases.tcyr` (degenerate loops, empty banks,
zero-length samples), `tests/nidhi.bcyr` (render-loop + interpolation benches),
`tests/nidhi.fcyr` (SFZ/SF2 parser fuzz — the highest-value fuzz targets).

**Serde:** every Rust type is `Serialize + Deserialize`. Cyrius has no serde. Roundtrip tests
become manual byte-layout round-trips (write struct fields to a buffer, read back, assert
equal) if persistence is needed — otherwise dropped.
