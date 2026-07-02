# Port Brief 16 — Serde & Testing in Cyrius

Read-only reconnaissance for the Rust→Cyrius port of **nidhi**. Covers two
cross-cutting Rust requirements and how they translate to Cyrius:

1. *"Every type is `Serialize + Deserialize` with roundtrip tests"*
2. *"Comprehensive tests + benchmarks"*

Sources: `cyrius/docs/{stdlib-modules,stdlib-reference}.md`,
`cyrius/docs/guides/cyrius-guide.md`, `cyrius/lib/bayan.cyr`,
`cyrius/CLAUDE.md`, `naad/` + `hisab/` (`tests/*.tcyr`, `tests/*.bcyr`,
`tests/*.fcyr`, `lib/bench.cyr`, `lib/assert.cyr`, `scripts/bench-history.sh`,
`bench-history.csv`), and `vidya/content/{serialization,jsonl_format,testing}`.

---

## 0. TL;DR — the two decisions nidhi must make

**Serde.** Cyrius has NO serde-the-trait. But cycc has a native
`#derive(Serialize)` code-gen that emits `Type_to_json(ptr, sb)` +
`Type_from_json(pairs) -> ptr`, backed by the opt-in JSON module
`lib/bayan.cyr` (`bayan_*` API, legacy aliases `json_*`). Str-field
deserialize was BROKEN until **cycc 6.3.25** — nidhi must pin
`cyrius >= 6.3.25` in `cyrius.cyml` and add `bayan` to `[deps].stdlib` if it
uses the derive on any struct with string fields.

Two viable strategies, pick per-struct:
- **(A) `#derive(Serialize)`** — declarative, one line, generates both
  directions. Good for config structs whose fields are all `i64`/`Str`/`f64`
  (floats stored as bit-patterns, serialized via bayan float). Roundtrip test =
  build → to_json → parse → from_json → to_json again → `str_eq`. This is what
  nidhi should use to satisfy the "every type Serialize+Deserialize" rule.
- **(B) Manual field-by-field** to/from `bayan_json_*`. Use when a struct has
  nested pointers (e.g. a `SampleBank` owning a vec of `Sample*`), enum-tagged
  unions (`LoopMode`, waveform), or needs a stable on-disk schema the derive
  can't express. You hand-write `sample_to_json(self, sb)` and
  `sample_from_json(pairs) -> ptr`.

> Sibling ports naad + hisab **dropped serde outright** ("Cyrius has no serde"
> — their `.tcyr` headers say so) because their types are pure numeric DSP
> state with no persistence requirement. nidhi cannot: its CLAUDE.md mandates
> "Every type must be Serialize + Deserialize" and "All types must have serde
> roundtrip tests". So nidhi is the first of these ports to actually exercise
> the `#derive(Serialize)` path — budget for the 6.3.25 pin and for the manual
> fallback on the container/enum types.

**Testing.** Cyrius has a real test/bench/fuzz harness discipline:
`.tcyr` (unit tests, `cyrius test`), `.bcyr` (benchmarks, `cyrius bench`),
`.fcyr` (fuzz, `cyrius fuzz`). There is **no `cyrius coverage` command** —
"coverage" in Cyrius means compile-time enum-exhaustiveness checking in
`switch`, nothing more. Bench results are scraped by `scripts/bench-history.sh`
into `bench-history.csv` (schema below) and rendered to `benchmarks.md`; that
CSV is the Rust-parity comparison ledger.

---

## 1. Serialization

### 1.1 The stdlib JSON module: `bayan`

- **Not in the default `std` group.** Opt-in: `include "lib/bayan.cyr"` in a
  harness, or add `"bayan"` to `[deps].stdlib` in `cyrius.cyml` for library
  builds. bayan 1.0.0 was carved out of stdlib at v6.1.25 (json / toml / cyml /
  csv / base64 / bigint(u256) / u128). Canonical names are `bayan_*`; legacy
  aliases `json_parse` / `json_get` / etc. still resolve.
- File: `cyrius/lib/bayan.cyr`. All functions return `i64` (a pointer, an int,
  or a negative error code — everything is i64).

**Parse / query API** (`bayan_json_parse` returns a *pairs vec* — a flat
key/value list, NOT the tagged tree):

| Function | Signature | Notes |
|---|---|---|
| `bayan_json_parse(src)` | `Str → vec` | Parse a JSON object into a pairs-vec |
| `bayan_json_get(pairs, key)` | `(vec, Str) → value/0` | Value ptr for key, or 0 |
| `bayan_json_get_int(pairs, key)` | `(vec, Str) → i64` | Convenience int getter |
| `bayan_json_build(pairs)` | `vec → Str` | Serialize a pairs-vec back to JSON |
| `bayan_json_parse_file_r(path)` | `Str → Result<vec, JsonError>` | file errors → `Err(JsonIoErr)` |

**Tagged-value API** (used by manual (de)serializers and by the derive's
`_from_json`). A "value" `v` is a heap object whose first word is a tag:

```
JTAG_NULL JTAG_BOOL JTAG_INT JTAG_FLOAT JTAG_STR JTAG_ARR JTAG_OBJ
```

| Constructor | Predicate | Extractor |
|---|---|---|
| `bayan_json_v_int_new(n)` | `bayan_json_v_is_int(v)` | `bayan_json_v_int(v) → i64` |
| `bayan_json_v_float_new(f)` | `bayan_json_v_is_float(v)` | `bayan_json_v_float(v) → f64bits` |
| `bayan_json_v_str_new(s)` | `bayan_json_v_is_str(v)` | `bayan_json_v_str(v) → Str ptr` |
| `bayan_json_v_bool_new(b)` | `bayan_json_v_is_bool(v)` | `bayan_json_v_bool(v)` |
| `bayan_json_v_arr_new()` | `bayan_json_v_is_arr(v)` | `bayan_json_v_arr_len(v)`, `bayan_json_v_arr_get(v,i)` |
| `bayan_json_v_obj_new()` | `bayan_json_v_is_obj(v)` | `bayan_json_v_obj_get(v,key)`, `_obj_len`, `_obj_key`, `_obj_val` |

Mutators: `bayan_json_v_arr_push(arr, val)`, `bayan_json_v_obj_set(obj, key, val)`.
Errors: `bayan_json_last_error()`, `bayan_json_last_error_pos()`.
`enum JsonError { JsonIoErr; JsonParseErr; JsonOther; }`.

> Gotcha: the parsed `Str` values live in the parse arena. When you deserialize
> a `Str` field into a heap struct, **copy the two Str words out** so they
> outlive the parse (the 6.3.25 derive fix does exactly this; the manual helper
> below shows the copy).

### 1.2 Strategy A — `#derive(Serialize)`

Declared exactly like `#derive(accessors)`, on a typed-field struct. The derive
generates BOTH `Type_to_json(ptr, str_builder)` and `Type_from_json(pairs) -> ptr`.

```cyr
include "lib/bayan.cyr"       # REQUIRED — derive calls bayan_json_*

#derive(Serialize)
struct AdsrConfig { attack; decay; sustain; release; }   # untyped fields = i64/f64bits
```

Generated surface (naming is `StructName_...`):
- `AdsrConfig_to_json(&cfg, sb)` — appends the JSON object to a `str_builder`.
- `AdsrConfig_from_json(pairs) -> ptr` — takes the `bayan_json_parse` result,
  returns a freshly-alloc'd struct pointer.

For a struct with `Str` fields you MUST be on cycc >= 6.3.25 (the Str
deserialize branch). Verified-fixed repro is
`cyrius/docs/development/issues/repros/derive-serialize-str-roundtrip.cyr`:

```cyr
#derive(Serialize)
struct Meta { title: Str; artist: Str; }
# ...
var m  = Meta { str_new("Song", 4), str_new("Band", 4) };
var sb = str_builder_new();  Meta_to_json(&m, sb);
var js = str_builder_build(sb);                    # {"title":"Song","artist":"Band"}
var rec = Meta_from_json(bayan_json_parse(js));    # 6.3.25+: reconstructs Str fields
```

**Caps to respect** (hard cycc limits, all cost real bugs when exceeded):
- Max **16 fields** per `#derive(accessors)`/`Serialize` struct for correct
  offsets historically; the field cap was raised (32→256) at v6.0.47, but stay
  conservative — nidhi's zone/instrument structs are wide.
- Max **64 `#derive` structs per source file**. If a flattened nidhi module
  exceeds this, split it or hand-write accessors on the overflow structs.

### 1.3 Strategy B — manual field-by-field

Use for containers, enums-as-int, and anything the derive can't model. Pattern
mirrors the 6.3.25 fix's hand-written helper. Floats are i64 bit-patterns, so
serialize them with the **float** value ctor and read them back with
`bayan_json_v_float` (returns the bits).

```cyr
include "lib/bayan.cyr"

# --- serialize: append `{"attack":<f>,"decay":<f>,...}` to a str_builder ---
fn adsr_to_json(self, sb) {
    var obj = bayan_json_v_obj_new();
    bayan_json_v_obj_set(obj, "attack",  bayan_json_v_float_new(AdsrConfig_attack(self)));
    bayan_json_v_obj_set(obj, "decay",   bayan_json_v_float_new(AdsrConfig_decay(self)));
    bayan_json_v_obj_set(obj, "sustain", bayan_json_v_float_new(AdsrConfig_sustain(self)));
    bayan_json_v_obj_set(obj, "release", bayan_json_v_float_new(AdsrConfig_release(self)));
    # str_builder append of the built value (or bayan_json_build on a pairs-vec)
    str_builder_append_str(sb, bayan_json_build_value(obj));
    return 0;
}

# --- helper: copy a Str field out of the parse arena into `dst` (2 words) ---
fn _get_str_field(dst, pairs, key) {
    var v = bayan_json_get(pairs, key);
    if (bayan_json_v_is_str(v) == 1) {
        var s = bayan_json_v_str(v);          # Str ptr (arena-owned)
        store64(dst,     load64(s));           # copy word 0 (ptr/len depends on Str repr)
        store64(dst + 8, load64(s + 8));       # copy word 1
    }
    return 0;
}

# --- helper: read an f64 (bit-pattern) field ---
fn _get_f64_field(pairs, key) {
    var v = bayan_json_get(pairs, key);
    if (bayan_json_v_is_float(v) == 1) { return bayan_json_v_float(v); }
    if (bayan_json_v_is_int(v)   == 1) { return f64_from(bayan_json_v_int(v)); }
    return 0;
}

# --- deserialize: alloc struct, fill each field, return ptr (or negative ERR) ---
fn adsr_from_json(pairs) {
    var self = alloc(sizeof(AdsrConfig));
    AdsrConfig_set_attack(self,  _get_f64_field(pairs, "attack"));
    AdsrConfig_set_decay(self,   _get_f64_field(pairs, "decay"));
    AdsrConfig_set_sustain(self, _get_f64_field(pairs, "sustain"));
    AdsrConfig_set_release(self, _get_f64_field(pairs, "release"));
    return self;
}
```

Enums (`LoopMode`, waveform, filter type) are already `var LOOPMODE_* = N;`
integer constants in the port — serialize the int directly with
`bayan_json_v_int_new`, deserialize with `bayan_json_v_int`, and validate the
range on the way in (return a negative `NIDHI_ERR_*` if out of range).

### 1.4 The roundtrip test (satisfies the "roundtrip tests" mandate)

A roundtrip test lives in a `.tcyr` and asserts serialize → parse →
deserialize → re-serialize is byte-stable:

```cyr
# In tests/serde.tcyr, per struct:
test_group("serde: AdsrConfig roundtrip");
var cfg = adsr_new(f64_from(1), f64_from(2), F64_HALF, f64_from(3));
var sb  = str_builder_new();
adsr_to_json(cfg, sb);                       # or AdsrConfig_to_json(&cfg, sb) for the derive
var js1 = str_builder_build(sb);

var cfg2 = adsr_from_json(bayan_json_parse(js1));
var sb2  = str_builder_new();
adsr_to_json(cfg2, sb2);
var js2 = str_builder_build(sb2);

assert(str_eq(js1, js2), "AdsrConfig serialize->deserialize->serialize stable");
```

Also assert individual fields survive (not just the whole string), so a
symmetric bug in both directions can't mask itself:

```cyr
assert_eq(AdsrConfig_attack(cfg2), AdsrConfig_attack(cfg), "attack survives roundtrip");
```

---

## 2. Testing

### 2.1 Commands (from `cyrius/CLAUDE.md` + `cyrius-guide.md`)

```sh
cyrius test <path.tcyr>   # resolve deps + compile + run ONE suite; also auto-discovers tests/tcyr/*.tcyr
cyrius bench              # discover + run benches/*.bcyr
cyrius fuzz               # discover + run fuzz/*.fcyr harnesses
cyrius soak [N]           # tests/scyr/*.scyr after the self-host loop
cyrius smoke              # tests/smcyr/*.smcyr fail-fast
```

**There is no `cyrius coverage`.** Do not invent one. Coverage = compile-time
enum-exhaustiveness only.

**Directory convention vs. what the ports actually do.** The guide says the
toolchain auto-scans `tests/tcyr/*.tcyr`, `benches/*.bcyr`, `fuzz/*.fcyr` and
that files elsewhere are "silently ignored" by discovery. But the sibling ports
(naad, hisab) keep their files flat in **`tests/*.tcyr` / `tests/*.bcyr` /
`tests/*.fcyr`** and invoke each explicitly by path:

```sh
cyrius test tests/oscillator.tcyr      # naad: explicit path, no auto-discovery
cyrius bench tests/hotpath.bcyr
```

naad's CLAUDE.md is explicit: *"`cyrius test tests/<mod>.tcyr` — run ONE suite
(explicit path — no auto-discovery)"*. **Recommendation for nidhi: follow the
sibling-port convention** — flat `tests/*.tcyr` + explicit-path invocation, one
`.tcyr` per source module (`tests/zone.tcyr`, `tests/loop_mode.tcyr`, …) plus a
`tests/serde.tcyr` for the roundtrip mandate. This matches how the parity
oracle repos are already run and CI'd.

> Toolchain concurrency hazard (from naad CLAUDE.md): `cyrius test/build/deps`
> all re-resolve deps and race on `cyrius.lock`. When running suites in
> parallel, serialize every toolchain call:
> `flock <scratch>/nidhi-build.lock cyrius test tests/zone.tcyr`.

### 2.2 `.tcyr` format

A `.tcyr` is a plain Cyrius program. It `include`s the project source modules
directly (NOT the distlib), calls `alloc_init()`, runs `test_group(...)` +
`assert*(...)`, and `syscall(60, assert_summary())` at the end. The exit code
IS the failure count (`assert_summary` returns `_assert_fail`), so exit 0 =
all-green. Assertions come from `lib/assert.cyr` (in the `std`/stdlib set):

```
assert(cond, name)          assert_eq(a, b, name)      assert_neq(a, b, name)
assert_gt / _lt / _gte / _lte(a, b, name)              assert_streq(a, b, name)
assert_nonnull(p, name)     test_group(name)           assert_summary() -> failcount
panic(msg)                  assert_fatal(cond, msg)
fail_after_n_allocs(n)      # OOM-injection allocator for error-path tests
```

Concrete skeleton (abbreviated from `naad/tests/oscillator.tcyr`):

```cyr
# tests/zone.tcyr — parity tests for src/zone.cyr.
# Ports every non-serde #[test] from rust-old/src/zone.rs; serde roundtrip is
# added separately in tests/serde.tcyr. f32 tolerances widened to f64.
include "src/error.cyr"
include "src/dsp_util.cyr"
include "src/zone.cyr"

alloc_init();

# f64 literals are IEEE-754 bit patterns declared as consts:
var C0_01 = 0x3F847AE147AE147B;   # 0.01

test_group("zone: key range clamps");
var z = zone_new(...);
assert(naad_is_err(zone_new_bad(...)), "invalid key range errors");
assert(f64_lt(f64_abs(f64_sub(zone_gain(z), F64_ONE)), C0_01), "unity gain default");

test_group("zone: velocity split");
# ... more groups ...

var rc = assert_summary();
syscall(60, rc);
```

Key porting conventions observed in the oracle `.tcyr` files:
- **Drop the serde `#[test]`s from the per-module suites** and re-home them in a
  dedicated `tests/serde.tcyr` (naad literally drops them; nidhi re-adds them
  because serde is required).
- **Errors are negative i64 codes**; test with `naad_is_err(x)` /
  `nidhi_is_err(x)` (a `x < 0` check) rather than `Result` matching.
- **Floats: no literals** — declare `var C0_5 = 0x3FE0...;` bit-patterns or use
  `f64_from(int)` and the `F64_ONE/HALF/TWO/TAU` constants; compare with
  `f64_lt(f64_abs(f64_sub(a,b)), tol)`, never `==`.
- Widen f32 Rust tolerances to f64 (the port is all f64).
- One `test_group` per logical concept; many `assert`s under it is fine.

### 2.3 Fuzz harness `.fcyr`

A `.fcyr` exposes `fn fuzz_main(data, len)` — `data` is a raw byte buffer, `len`
its length — returning 0 for OK, nonzero for a detected invariant break.
`cyrius fuzz` drives it with arbitrary inputs. The file ALSO ships a `main()`
self-test that feeds known-good byte patterns through `fuzz_main` so it runs
green as a plain program too. See `hisab/tests/hisab.fcyr` for a full example;
`naad/tests/naad.fcyr` is the minimal stub.

Idioms from `hisab.fcyr`:
- Read typed values out of the byte buffer: `load64(data + offset)` for an i64
  or an f64 bit-pattern; clamp/mask integers to avoid pathological loops
  (`v & 0x7FFFFFFF`).
- Guard invariants only when inputs are finite (helpers `fuzz_is_nan`,
  `fuzz_is_finite` inline the exponent check `((bits >> 52) & 0x7FF) == 0x7FF`).
- On a violated invariant: `println("fuzz: FAIL ...")` and `return 1`.
- Dispatch by length in `fuzz_main` (each target needs N bytes).

Template for nidhi (e.g. fuzz the SFZ/SF2 parser and the loop/crossfade math):

```cyr
# fuzz/nidhi.fcyr — no crash + key invariants on arbitrary bytes.
include "src/error.cyr"
include "src/loop_mode.cyr"
include "src/zone.cyr"
include "src/sfz.cyr"

fn fuzz_read_f64(data, off) { return load64(data + off); }
fn fuzz_is_finite(x) { if (((x >> 52) & 0x7FF) == 0x7FF) { return 0; } return 1; }

# Target: loop-point resolution never returns a start > end for finite inputs.
fn fuzz_loop_points(data) {
    var start = load64(data) & 0x7FFFFFFF;
    var end   = load64(data + 8) & 0x7FFFFFFF;
    var lm    = load64(data + 16) & 3;             # LoopMode in 0..3
    var r = loop_resolve(lm, start, end);          # must not crash
    if (r < 0) { return 0; }                        # negative = clean error, fine
    if (loop_start(r) > loop_end(r)) {
        println("fuzz: FAIL loop start > end");
        return 1;
    }
    return 0;
}

fn fuzz_main(data, len) {
    if (len >= 24) { var rc = fuzz_loop_points(data); if (rc != 0) { return rc; } }
    # Feed the raw bytes to the text/binary parsers — they must reject, not crash.
    if (len >= 4)  { sfz_parse(data, len); }        # return value ignored; crash = fail
    return 0;
}

fn main() {
    alloc_init();
    var d = alloc(64);
    store64(d, 0);  store64(d + 8, 44100);  store64(d + 16, 1);   # forward loop
    if (fuzz_main(d, 24) != 0) { println("fuzz: FAIL self-test"); return 1; }
    println("fuzz: ok");
    return 0;
}
var r = main(); syscall(60, r);
```

---

## 3. Benchmarks

### 3.1 `.bcyr` format + `lib/bench.cyr` API

A `.bcyr` is a plain program that uses `lib/bench.cyr` (in the stdlib set).
Core API:

```
bench_new(name) -> b            # 48-byte state {name,start,elapsed,iters,min,max}
bench_start(b) / bench_stop(b)  # per-iter timing (~240 ns clock overhead per pair)
bench_batch_start(b) / bench_batch_stop(b, count)   # amortized — for sub-1us ops
bench_run(b, fnptr, n)          # call fnptr n times, timing each
bench_report(b)                 # prints the result line the CSV scraper parses
bench_avg_ns / _min_ns / _max_ns / _iterations / _total_ns(b)
```

Two idiomatic shapes, both from naad:

**Batch loop (sub-1us hot path)** — `naad/tests/naad.bcyr`:
```cyr
fn main() {
    alloc_init();
    var b = bench_new("noop");
    bench_batch_start(b);
    var i = 0;
    while (i < 1000000) { bench_noop(); i = i + 1; }
    bench_batch_stop(b, 1000000);
    bench_report(b);
    return 0;
}
var r = main(); syscall(60, r);
```

**Fixture + fn-pointer dispatch** — `naad/tests/hotpath.bcyr` (nidhi's model:
one bench per per-sample render function):
```cyr
include "src/error.cyr"
include "src/engine.cyr"
include "src/zone.cyr"

fn bench(name, fp, n) { var b = bench_new(name); bench_run(b, fp, n); bench_report(b); return 0; }

var _voice = 0;
fn b_render_sample() { return engine_render_sample(_voice); }   # the hot path

fn main() {
    alloc_init();
    _voice = engine_new_voice(...);
    bench("engine render_sample", &b_render_sample, 200000);
    bench("zone process (svf+env)", &b_zone_process, 200000);
    return 0;
}
var r = main(); syscall(60, r);
```

> Portability landmine baked into `lib/bench.cyr`: the `clock_gettime` syscall
> number **228 must be a compile-time literal at every call site** (never a
> `var`) or the macOS/Windows reroute breaks and all timings read 0. nidhi
> should not touch this — just `include`/use the stdlib `bench`.

Run: `cyrius bench tests/hotpath.bcyr` (explicit path, sibling-port style) or
`cyrius bench` for `benches/*.bcyr` auto-discovery.

### 3.2 `bench-history.csv` — the Rust-parity ledger

`scripts/bench-history.sh` (copy hisab's, NOT naad's — naad's is a stale
`cargo bench`/criterion leftover) runs the suite, parses each
`<name> ... <median><unit>` line from `bench_report`, normalizes to ns, and
**appends** one row per benchmark to `bench-history.csv`, then regenerates
`benchmarks.md` with a 3-point (baseline → mid → current) trend table.

CSV schema (exact header, from `hisab/bench-history.csv`):
```
timestamp,commit,branch,benchmark,estimate_ns
2026-05-28T23:45:25Z,e1f8f4c,main,vec3_add:,189000.0000
```
- `timestamp` = `date -u +%Y-%m-%dT%H:%M:%SZ`
- `commit` = `git rev-parse --short HEAD`
- `branch` = `git branch --show-current`
- `benchmark` = the name passed to `bench_new(...)` (the scraper keeps the
  trailing `:` printed by `bench_report`)
- `estimate_ns` = median per-iter time, normalized to ns (ps/us/ms/s all folded
  to ns by the script's `normalize_to_ns`)

**How this gives Rust parity.** The header comment in hisab's script frames it
as tracking the port's numbers over time; nidhi should seed the CSV with the
Rust baseline (run `rust-old/`'s criterion benches, drop their median-ns into a
first CSV row per equivalent bench name) so the trend table shows
`Rust baseline → Cyrius current` deltas per operation. The Python block in the
script renders `**-43%**`-style deltas vs. the baseline column — that column IS
the Rust number when you seed it that way. Keep bench names identical to the
Rust criterion IDs so rows line up.

Invoke: `./scripts/bench-history.sh` (default suite `tests/hisab.bcyr` — nidhi
sets its default to `tests/hotpath.bcyr`), or
`./scripts/bench-history.sh "" tests/other.bcyr` for a specific suite.

---

## 4. Concrete checklist for nidhi

1. Pin `cyrius >= 6.3.25` in `cyrius.cyml` (`[package].cyrius`) — required for
   `#derive(Serialize)` Str-field deserialize.
2. Add `"bayan"` to `[deps].stdlib` in `cyrius.cyml` (JSON is opt-in post-v6.1.25).
3. For each config struct: prefer `#derive(Serialize)` (all-`i64`/`Str`/`f64`
   fields, ≤16 fields, ≤64 derives/file). Fall back to manual
   `X_to_json`/`X_from_json` (§1.3) for containers (`SampleBank`, `Instrument`),
   enum-tagged fields (`LoopMode`, waveform, filter type), and any wide struct.
4. `tests/serde.tcyr`: one roundtrip `test_group` per type — serialize → parse →
   deserialize → re-serialize → `assert(str_eq(js1, js2))` + per-field asserts.
5. `tests/<module>.tcyr` per source module (flat in `tests/`, explicit-path
   run), porting the non-serde Rust `#[test]`s; negative errors via
   `nidhi_is_err`, floats via bit-patterns + `f64_*` tolerance compares.
6. `fuzz/nidhi.fcyr` (or `tests/nidhi.fcyr`): `fuzz_main(data,len)` over the
   parsers (SFZ/SF2) and the loop/crossfade math + a `main()` self-test.
7. `tests/hotpath.bcyr`: per-sample render benches (engine, zone, envelope,
   stretch), fixture + fn-ptr dispatch; names matching the Rust criterion IDs.
8. Copy **hisab's** `scripts/bench-history.sh`; seed `bench-history.csv` with
   the Rust baseline row; regenerate `benchmarks.md`.
9. Do NOT expect a `cyrius coverage` command — it doesn't exist.
10. `flock` all `cyrius test/build/bench` calls when running in parallel
    (`cyrius.lock` race).

## 5. Key source paths (absolute)

- JSON API: `/home/macro/Repos/cyrius/lib/bayan.cyr` (fns ~line 1992–2490)
- Derive-Serialize repro + fix note:
  `/home/macro/Repos/cyrius/docs/development/issues/repros/derive-serialize-str-roundtrip.cyr`,
  `/home/macro/Repos/cyrius/docs/development/issues/archived/2026-07-01-derive-serialize-str-field-deserialize-broken.md`
- Derive syntax: `/home/macro/Repos/cyrius/docs/guides/cyrius-guide.md` (§Derive Accessors ~line 335)
- `#derive(accessors)` real struct: `/home/macro/Repos/naad/src/osc_core.cyr` (line 90)
- Commands/layout: `/home/macro/Repos/cyrius/docs/guides/cyrius-guide.md` (~lines 440–490), `/home/macro/Repos/cyrius/CLAUDE.md`
- `.tcyr` example: `/home/macro/Repos/naad/tests/oscillator.tcyr`
- `.fcyr` example: `/home/macro/Repos/hisab/tests/hisab.fcyr`
- `.bcyr` examples: `/home/macro/Repos/naad/tests/hotpath.bcyr`, `/home/macro/Repos/naad/tests/naad.bcyr`
- Bench lib: `/home/macro/Repos/hisab/lib/bench.cyr`
- Assert lib: `/home/macro/Repos/hisab/lib/assert.cyr`
- Bench history: `/home/macro/Repos/hisab/scripts/bench-history.sh`, `/home/macro/Repos/hisab/bench-history.csv`, `/home/macro/Repos/hisab/benchmarks.md`
- stdlib index / bayan fold: `/home/macro/Repos/cyrius/docs/stdlib-modules.md`, `/home/macro/Repos/cyrius/docs/stdlib-reference.md` (§json.cyr ~line 325)
