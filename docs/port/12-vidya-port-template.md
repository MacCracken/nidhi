# 12 — vidya as the end-to-end Rust→Cyrius port template

> ⚠ **Historical brief (2026-07), kept as written. nidhi did not end up like this.** A future port
> that copies this file should know where nidhi diverged:
> - **Layout**: nidhi kept **one `.cyr` per Rust module** (`src/*.cyr`, 14 files bundled by
>   `cyrius distlib` into `dist/nidhi.cyr`), not the single-file layout §1 recommends.
> - **`lib/` and `cyrius.lock` are committed** in nidhi, not gitignored.
> - **`rust-old/` was retired in 2.0.4**, not kept frozen in the tree (§6). Before removing it,
>   nidhi captured what it needed: the oracle's build identity and lockfile
>   ([`oracle-build-identity.md`](oracle-build-identity.md)), 43 golden assertions against a live
>   oracle build (`tests/golden.tcyr`), and the full source in git history. Plan that retirement
>   from the start; a live `rust-old/` cost 377 MB.

Read-only reconnaissance brief for the nidhi port. Everything below is drawn
from `/home/macro/Repos/vidya`, a **completed** Rust→Cyrius port (Rust v1.5.0 →
Cyrius v2.0.0, now at v2.7.3). Use it as the concrete template: what the repo
looks like when "done", what the `cyrius` toolchain produced vs what was hand
written, how Rust-vs-Cyrius parity benchmarking is recorded, the CI/release
workflows, and how the toolchain is pinned.

> Cyrius reminder (applies throughout): everything is `i64`. Structs are heap
> blocks of 8-byte slots. Floats are `f64` **bit-patterns** carried in `i64`
> and manipulated only through `f64_*` intrinsics. Errors are negative integer
> codes / sentinel `0`. No serde, no generics, no trait objects, no closures
> (function pointers via `&fn` + `lib/fnptr.cyr`).

---

## 1. Exact repo layout after a completed port

vidya's tree today (`/home/macro/Repos/vidya`), annotated with origin:

```
vidya/
├── VERSION                     # "2.7.3" — single source of truth (6 bytes)
├── cyrius.cyml                 # HAND-BUILT manifest (replaces Cargo.toml). See §5
├── cyrius.lock                 # cyrius-GENERATED sha256 lock of vendored lib/*.cyr; GITIGNORED under 5.11+
├── CLAUDE.md                   # HAND-REWRITTEN — cyrius toolchain, not cargo. See §7
├── CHANGELOG.md                # HAND — Keep-a-Changelog; the port is one big [2.0.0] entry
├── README.md CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md LICENSE
├── BENCHMARKS.md               # HAND — Cyrius-vs-Rust parity table. See §4
├── bench-history.csv           # HAND/appended — Cyrius bench history (date,version,benchmark,mean_ns,min_ns,max_ns,iters)
├── bench-history-rust.csv      # FROZEN — old Rust criterion baseline (benchmark,mean_ns) — the parity target
├── src/
│   └── main.cyr                # HAND-WRITTEN port — the ENTIRE program in ONE file (1920 lines, 65 KB)
├── tests/
│   ├── vidya.tcyr              # HAND — cyrius-native tests (`cyrius test`)
│   ├── vidya.bcyr              # HAND — cyrius-native benchmarks (`cyrius bench`)
│   └── test.sh                 # HAND — legacy pipe-through-cc shim
├── lib/                        # cyrius-GENERATED (vendored stdlib), GITIGNORED — rehydrated by `cyrius deps`
│   ├── string.cyr str.cyr alloc.cyr vec.cyr hashmap.cyr io.cyr fs.cyr
│   ├── fmt.cyr args.cyr syscalls.cyr tagged.cyr fnptr.cyr regex.cyr net.cyr
│   ├── bayan.cyr               # 6.1.x: json+toml+base64 bundled here (were separate before)
│   ├── bench.cyr assert.cyr    # test/bench harness the .tcyr/.bcyr use
│   ├── sakshi.cyr              # git dep (tracing) — vendored via [deps.sakshi]
│   └── vyakarana.cyr sandhi.cyr … (~90 modules total)
├── scripts/
│   ├── bench-history.sh        # HAND — snapshot `cyrius bench` to target/bench-history/<ts>-<sha>.txt
│   ├── validate-content.sh     # HAND — content gate (vidya-specific; nidhi likely won't need this)
│   └── version-bump.sh         # HAND — writes VERSION + stamps CHANGELOG
├── docs/
│   ├── adr/0001-port-from-rust-to-cyrius.md   # HAND — the port decision record (READ THIS; it's the playbook)
│   ├── development/state.md    # HAND — volatile state: version, cyrius pin, binary size, dep pins
│   ├── development/roadmap.md  BENCHMARKS.md architecture/overview.md …
├── build/vidya                 # cyrius-GENERATED static ELF binary (gitignored)
├── .github/workflows/
│   ├── ci.yml                  # HAND — install cyrius, deps, lint, build, smoke, test, bench, content, security, docs
│   └── release.yml             # HAND — tag-triggered: CI gate → build x86_64(+aarch64) → GitHub release
└── rust-old/                   # THE FROZEN RUST CRATE — moved here wholesale. See §6
    ├── Cargo.toml Cargo.lock
    ├── LINES_OF_RUST.txt        # a single number: "2396" — the pre-port Rust LOC. See §6
    ├── src/*.rs benches/benchmarks.rs examples/ tests/
    └── content -> ../content    # symlink (vidya-specific; N/A for nidhi)
```

Key structural facts:
- **One `.cyr` file holds the whole program.** vidya did not port module-per-module
  into separate files. All 11 Rust source modules collapsed into `src/main.cyr`,
  organized by big `# ===...===` banner-comment section headers (see §3). nidhi's
  14 Rust modules should likewise become one `src/nidhi.cyr` (or `src/main.cyr` if
  it grows a CLI; a pure library may instead ship `src/lib.cyr` — but the cyml
  `[build] entry` still points at one file).
- **`lib/` and `cyrius.lock` are gitignored build artifacts** (rehydrated by
  `cyrius deps` / `cp` from `$HOME/.cyrius/lib`). They are NOT committed.
- **`rust-old/` is committed and frozen**, gitignored only from CI (`rust-old/target/`).

---

## 2. What "cyrius port" (the toolchain) produced vs what was hand-built

There is **no automatic Rust→Cyrius transpiler**. "Porting" = a human rewrites
the logic in `.cyr`. The cyrius toolchain only produces build/dep artifacts:

| Artifact | Produced by | Notes |
|---|---|---|
| `src/main.cyr` | **HAND-WRITTEN** | The port itself. Rewrite of all Rust logic. |
| `cyrius.cyml` | **HAND-WRITTEN** | Manifest. Replaces `Cargo.toml`. |
| `lib/*.cyr` | `cyrius deps` (+ `cp -rL $HOME/.cyrius/lib/* lib/`) | Vendored stdlib; gitignored. |
| `cyrius.lock` | `cyrius deps` | sha256 of vendored libs; gitignored under 5.11+ (empty). |
| `build/vidya` | `cyrius build src/main.cyr build/vidya` | Static ELF. `xxd -l 4` starts `7f45 4c46`. |
| `tests/*.tcyr`, `*.bcyr` | **HAND-WRITTEN** | Native test/bench sources (see §4). |
| CHANGELOG / BENCHMARKS / ADR / docs | **HAND-WRITTEN** | |

So for nidhi the deliverable is overwhelmingly hand-authored `.cyr` code plus a
handful of hand-authored manifest/CI/doc files. The toolchain's only output is
the vendored `lib/`, the lock, and the compiled binary.

---

## 3. `src/main.cyr` idioms to copy verbatim

The nidhi engine is float-heavy DSP, so the struct/float idioms matter more here
than they did for vidya (which was string/TOML heavy). Concrete patterns pulled
from `vidya/src/main.cyr`:

**File header + includes** (top of file). Every stdlib module used must be
`include`d explicitly; the compiler does not auto-resolve transitive stdlib:
```
include "lib/string.cyr"
include "lib/alloc.cyr"
include "lib/vec.cyr"
include "lib/fs.cyr"
include "lib/fnptr.cyr"    # needed for &fn function pointers (round-robin, callbacks)
include "lib/math.cyr"     # f64_min/max/clamp/lerp/sqrt/parse (nidhi will lean on this)
```

**Enums are plain integer constants** (loop_mode.rs → this exactly):
```
enum LoopMode { ONE_SHOT = 0; FORWARD = 1; PING_PONG = 2; REVERSE = 3; LOOP_SUSTAIN = 4; }
```
Match arms become `if` chains returning by value:
```
fn loop_mode_name(m) {
    if (m == ONE_SHOT) { return "OneShot"; }
    if (m == FORWARD)  { return "Forward"; }
    ...
    return "Unknown";
}
```

**Structs — two idioms, pick per case.**

(a) *Manual layout* (vidya's dominant style; explicit, portable, self-documenting):
```
# Concept layout: { id, title, ... } — 9 fields x 8 bytes = 72 bytes
fn concept_new() {
    var c = alloc(72);
    store64(c, 0);       # id: Str
    store64(c + 8, 0);   # title: Str
    ...
    return c;
}
fn concept_id(c) { return load64(c); }
fn concept_title(c) { return load64(c + 8); }
```
Field access is `load64(ptr + offset)` / `store64(ptr + offset, v)`. **You count
offsets by hand.** Comment the byte layout above every `_new`.

(b) *`#derive(accessors)`* (used in `lib/agnosys.cyr`; generates getters/setters —
lays each field at an i64 slot):
```
#derive(accessors)
struct mac_profile { agent_type; selinux_ctx; apparmor_name; }   # fields are UNTYPED; single line only
# auto-generates:  mac_profile_agent_type(p) / mac_profile_set_agent_type(p, v)
fn mac_profile_new(agent_type): i64 {
    var p = alloc(24);
    mac_profile_set_agent_type(p, agent_type);
    ...
    return p;
}
```
Caveat baked into the codebase: struct/enum bodies **must be on a single line**
(the parser trips on multi-line struct-literal disambiguation), and
`#derive(accessors)` lays every field at an 8-byte slot — so a struct holding a
`f64` field stores the **bit-pattern** at that slot (fine, since floats are i64
bit-patterns anyway). For nidhi's hot structs (Voice, Zone, Sample), manual
layout (a) is the safer bet — you control alignment and can pack a mono/stereo
flag, sample-rate, loop points, and an f64 phase accumulator with explicit
offsets.

**Floats — the single most important nidhi mapping.** Cyrius has no float type;
`f64` values are i64 bit-patterns produced/consumed by compiler intrinsics.
`ganita.cyr` uses them exactly this way (`# f64 as i64 bit patterns`). Core ops
(compiler builtins — do NOT try to `include` them, they emit inline):
```
f64_from(int)      # int  -> f64 bit-pattern   (e.g. f64_from(1) == 1.0)
f64_to(fbits)      # f64  -> int (truncate)
f64_add(a,b) f64_sub(a,b) f64_mul(a,b) f64_div(a,b) f64_neg(x) f64_sqrt(x)
f64_lt/le/gt/ge/eq(a,b)  # comparisons -> 0/1
```
Plus `lib/math.cyr` helpers you WILL want for DSP:
`f64_min f64_max f64_clamp(x,lo,hi) f64_lerp(a,b,t) f64_sign f64_trunc f64_fract
f64_parse f64_parse_ok` and polyfills `_f64_exp_polyfill / _f64_ln_polyfill /
_f64_log2_polyfill`. Real usage from `ganita.cyr`:
```
sum = f64_add(sum, f64_mul(ganita_mat_get(a,i,p), ganita_mat_get(b,p,j)));   # dot product
store64(c + 16 + i*8, f64_add(va, vb));                                       # store f64 into slot
```
For nidhi this means: sample data is an array of f64 bit-patterns (`store64` at
`base + i*8`); interpolation, gain, pitch ratio, envelope, filter coefficients
all become `f64_*` chains; a phase accumulator is an f64 bit-pattern advanced by
`f64_add(phase, ratio)` each frame. **There is no `a * b` on floats — it is
always `f64_mul(a, b)`.** Watch for accidental integer arithmetic on what should
be float slots; that is the #1 port bug class in this style.

**Strings**: heap `Str` objects (`str_from(cstr)`, `str_data`, `str_len`,
`str_new(ptr,len)`), plus `to_cstr(s)` to get a null-terminated key, and
`str_builder_new / str_builder_add_cstr / str_builder_build` for assembly.
`streq(a,b)==1` for cstr equality; `str_eq_cstr(str, cstr)==1` for Str-vs-cstr.

**Errors — negative-int / sentinel-0 convention** (no Result type). vidya's
`NidhiError` equivalent handling:
```
if (l1 < 0) { return 0 - 1; }              # -1 = error code
if (ex == 0) { return 0 - 1; }             # 0 = "not found" sentinel
...
sys_write(STDERR_FD, "error: topic not found: ", 24);   # human message to stderr
```
`0 - 1` is the idiom for `-1` (literal negatives are written this way throughout).
Map nidhi's `NidhiError` variants to distinct negative codes (e.g.
`-1 SampleNotFound, -2 InvalidZone, -3 InvalidParameter, -4 Playback,
-5 ImportError`) and emit the descriptive string to stderr at the call site.

**Entry point** (bottom of file — not a `main` attribute, an explicit call +
syscall exit):
```
fn main() {
    alloc_init();     # ALWAYS first — bump allocator init
    args_init();      # if using argv
    ...
    return 0;
}
var exit_code = main();
syscall(SYS_EXIT, exit_code);
```
For a pure library (nidhi is a `lib` crate), there may be no CLI `main`; the
`.tcyr`/`.bcyr` files provide their own `main` and the library functions are
`include`d. Decide early whether nidhi ships a demo/CLI `main` or is
consumed only via include by dhvani.

---

## 4. How benchmarks encode Rust-vs-Cyrius parity ("the target to beat")

This is the core of the "parity to benchmark against" goal. Three artifacts work
together:

1. **`bench-history-rust.csv`** — the FROZEN old-Rust criterion baseline, format
   `benchmark,mean_ns`. This is the number the port must be measured against. It
   is captured **once**, before/at the port, from the Rust crate's criterion
   suite, and never changes:
   ```
   benchmark,mean_ns
   registry_get_hit,16.60
   search_text_hit,30496.23
   load_single_concept,123323.80
   load_all_content,3830120.58
   ```

2. **`bench-history.csv`** — the Cyrius bench history, richer schema
   `date,version,benchmark,mean_ns,min_ns,max_ns,iters`, appended each release:
   ```
   date,version,benchmark,mean_ns,min_ns,max_ns,iters
   2026-04-08,1.6.0-cyr,load_concept,28000,24000,134000,100
   2026-04-08,1.6.0-cyr,reg_get_hit,493,461,6000,10000
   ```

3. **`BENCHMARKS.md`** — the hand-written narrative that puts them side by side in
   a ratio table and interprets each result honestly (wins AND losses):
   ```
   | Benchmark   | Cyrius v2.0 (ns) | Rust v1.5.0 (ns) | Ratio        |
   | reg_get_hit | 493              | 17               | 29x slower   |
   | search_text | 4,000            | 30,496           | 7.6x faster  |
   | load_concept| 28,000           | 123,324          | 4.4x faster  |
   ```
   With takeaways ("Rust's SipHash HashMap beats Cyrius FNV-1a by ~30×; expected.
   Cyrius's hand-rolled parser beats serde by 4×."). **Parity is not "Cyrius must
   win everywhere" — it is "measure the same operations both ways and explain the
   deltas."**

**The Cyrius bench harness** — `tests/vidya.bcyr`, run via `cyrius bench`, using
`lib/bench.cyr` (nanosecond precision via `clock_gettime(CLOCK_MONOTONIC_RAW)`).
Structure to copy:
```
include "lib/bench.cyr"
include "lib/fnptr.cyr"          # bench_run takes a &fn function pointer

fn bench_reg_get_hit() { map_get(_b_reg, "strings"); return 0; }   # one op, no args

fn main() {
    alloc_init();
    # ... build fixtures into globals (_b_reg etc.) ...
    var benches = vec_new();
    var b3 = bench_new("reg_get_hit");
    bench_run(b3, &bench_reg_get_hit, 10000);   # name, &fn, iters
    vec_push(benches, b3);
    ...
    bench_report_all(benches);
    return 0;
}
var r = main();
syscall(60, r);
```
For nidhi, mirror the Rust criterion benches in `benches/benchmarks.rs` (voice
render, buffer fill, interpolation, filter processing, engine scaling by voice
count). Each Rust `#[bench]` gets: (a) a criterion run recorded once into
`bench-history-rust.csv`, and (b) a matching `bench_*` fn in `tests/nidhi.bcyr`.
Keep iteration counts explicit per benchmark (100 for meso, 10 for macro,
10000 for micro — matching AGNOS micro/meso/macro tiers documented in
BENCHMARKS.md).

**Cyrius tests** — `tests/vidya.tcyr`, run via `cyrius test`, using
`lib/assert.cyr`:
```
include "lib/assert.cyr"
fn main() {
    alloc_init();
    test_group("loop_mode");
    assert(streq(loop_mode_name(1), "Forward") == 1, "forward name");
    assert_eq(loop_mode_parse("forward"), 1, "parse forward");
    return assert_summary();       # returns nonzero on any failure
}
var exit_code = main();
syscall(60, exit_code);
```
Note the tcyr/bcyr files **re-include or inline the functions under test** (they
don't import `src/main.cyr` — there's no module import; vidya duplicated the
small helpers it needed). For nidhi, factor testable pure functions so they can
be `include`d by both `src/` and `tests/`, or accept the inline-duplication that
vidya used for small helpers.

**Snapshotting**: `scripts/bench-history.sh` writes each run to
`target/bench-history/<ts>-<sha>.txt`. (vidya's copy still shells `cargo bench`
— stale; for nidhi it must call `cyrius bench`.)

---

## 5. Toolchain pinning — the `[package].cyrius` field

`cyrius.cyml` is the manifest (TOML). The **`cyrius` key under `[package]` pins
the exact toolchain version** — this is the analog of `rust-toolchain.toml`.
vidya's full manifest:
```toml
[package]
name = "vidya"
version = "${file:VERSION}"          # reads the VERSION file — never edit version here
description = "..."
license = "GPL-3.0-only"
repository = "https://github.com/MacCracken/vidya"
language = "cyrius"
cyrius = "6.1.41"                     # <-- TOOLCHAIN PIN. CI greps this to install the compiler.

[build]
entry = "src/main.cyr"               # single entry file
output = "build/vidya"

[deps]
stdlib = [ "syscalls", "string", "alloc", "str", "fmt", "vec", "hashmap",
           "io", "fs", "tagged", "bayan", "fnptr", "args", "regex", "net", ... ]

[deps.sakshi]                        # git deps like Cargo git deps
git = "https://github.com/MacCracken/sakshi.git"
tag = "2.2.10"
modules = ["dist/sakshi.cyr"]

[deps.vyakarana]
git = "..."; tag = "2.2.3"; modules = ["dist/vyakarana.cyr"]
```
- `version = "${file:VERSION}"` — the manifest never carries a literal version;
  it interpolates the `VERSION` file. `version-bump.sh` only writes `VERSION` +
  stamps `CHANGELOG`.
- `cyrius = "6.1.41"` — CI extracts it with
  `grep '^cyrius = ' cyrius.cyml | sed 's/cyrius = "\(.*\)"/\1/'` and installs
  exactly that toolchain. `docs/development/state.md` documents the pin
  rationale (e.g. "ecosystem wrapper is 6.2.0 — build with `--strict-pin`").
- `[deps] stdlib` lists **every** stdlib module to vendor (explicit — transitive
  resolution has gaps, hence the long list + comments).
- nidhi's stdlib list will be smaller and DSP-focused: `syscalls, string, alloc,
  str, fmt, vec, io, fs, fnptr, math` (+`bench`/`assert` for tests). It will NOT
  need `net/tls/sandhi/regex/bayan` unless nidhi grows a CLI/HTTP surface. WAV/
  file IO (nidhi's `io.rs`, `shravan` dep) maps to `lib/fs.cyr` + hand-written
  RIFF/WAV parsing (no shravan equivalent — hand-roll it, like vidya hand-rolled
  its TOML parser).

---

## 6. `rust-old/` and `LINES_OF_RUST.txt`

> nidhi retired its `rust-old/` in 2.0.4 (see the note at the top of this file). What follows is
> vidya's convention as of 2026-07.

When the port lands, the **entire Rust crate is moved wholesale into
`rust-old/`** (per AGNOS first-party-standards convention for ported crates) and
frozen as a historical artifact:
```
rust-old/
├── Cargo.toml  Cargo.lock
├── LINES_OF_RUST.txt      # contents: "2396" — just the integer LOC of the pre-port Rust
├── src/*.rs               # all the original modules, untouched
├── benches/benchmarks.rs  # the original criterion harness (source of bench-history-rust.csv)
├── tests/  examples/
```
- **`LINES_OF_RUST.txt`** is a one-line file containing the total line count of
  the retired Rust source (vidya: `2396`). It is the "before" number cited
  everywhere (CHANGELOG, BENCHMARKS, ADR: "600 lines of Cyrius replacing 2,396
  lines of Rust"). It is the headline density metric of the port. For nidhi,
  `find src -name '*.rs' | xargs wc -l` currently totals **7180** lines across 14
  modules — that number goes into `rust-old/LINES_OF_RUST.txt`.
- `rust-old/` is committed but CI-excluded (`rust-old/target/` gitignored). The
  ADR notes it as "dead weight preserved per convention; readers may briefly
  mistake it for current code" — mitigate with a `(no longer a Rust crate)` note
  in CLAUDE.md.
- The pre-port criterion run of `rust-old/benches/benchmarks.rs` is what
  populates `bench-history-rust.csv` — **run it once before deleting/freezing to
  capture the baseline.**

---

## 7. CI / release workflow contents

**`.github/workflows/ci.yml`** (jobs: build-and-test, content, security, docs;
`workflow_call`-able so release can reuse it). The load-bearing steps for a
generic Cyrius port (drop vidya's content/security specifics):

1. **Install Cyrius toolchain** — pipe upstream install.sh, version from the
   manifest pin:
   ```bash
   CYRIUS_VERSION="${CYRIUS_VERSION:-$(grep '^cyrius = ' cyrius.cyml | head -1 | sed 's/cyrius = "\(.*\)"/\1/')}"
   curl -sSf https://raw.githubusercontent.com/MacCracken/cyrius/main/scripts/install.sh | \
     CYRIUS_VERSION="${CYRIUS_VERSION}" sh
   echo "$HOME/.cyrius/bin" >> "$GITHUB_PATH"
   ```
2. **Resolve dependencies** — stage stdlib into project `lib/`, then git deps:
   ```bash
   mkdir -p lib
   cp -rL "$HOME/.cyrius/lib/"* lib/     # cc reads ./lib/ specifically
   cyrius deps
   ```
3. **Lint** — per-file (`cyrius lint <f>` for each `src/*.cyr`); the cosmetic
   "exceeds 120 characters" warning is tolerated, everything else hard-fails.
4. **Build** — `cyrius build src/main.cyr build/vidya` then report byte size.
5. **Verify ELF** — `xxd -l 4 build/vidya | grep -q "7f45 4c46"`.
6. **Smoke test** — run the binary on representative inputs.
7. **Test** — `cyrius test` (`continue-on-error: true` in vidya).
8. **Bench** — `cyrius bench` (`continue-on-error: true`).

`docs` job checks required files exist (README, CHANGELOG, VERSION, LICENSE,
cyrius.cyml, …) and verifies `VERSION` string appears in CHANGELOG. `security`
job greps for dangerous syscall patterns.

**`.github/workflows/release.yml`** — triggered on `v1.2.3` **or** `1.2.3` tags:
- `ci` job: `uses: ./.github/workflows/ci.yml` (CI is the gate).
- `build` job: verify `VERSION` == tag (strip optional leading `v`); install
  toolchain; `cyrius deps`; `cyrius build` x86_64; best-effort
  `cyrius build --aarch64` (skip with a warning if `cc5_aarch64` absent);
  archive `tar czf` binary + assets; `sha256sum > SHA256SUMS`.
- `release` job: extract the tag's CHANGELOG section via `awk`, publish with
  `softprops/action-gh-release@v2`; `prerelease` when tag starts `0.`/`v0.`.

nidhi already has `.github/workflows/{ci.yml,release.yml}` — these are the
**Rust/cargo** versions and must be rewritten to the Cyrius shape above at port
time (same as vidya replaced its cargo CI).

---

## 8. "Done" checklist for the nidhi port

Everything that must exist for the nidhi Rust→Cyrius port to match vidya's
completed state:

**Source & build**
- [ ] `src/nidhi.cyr` (or `src/main.cyr`) — all 14 Rust modules ported into one
      file, organized by `# ===` banner sections (engine, voice, sample, zone,
      instrument, envelope, loop_mode, effect_chain, stretch, capture, sf2, sfz,
      io, error). ~7180 Rust lines → target a comparable-or-smaller `.cyr`.
- [ ] All float math converted to `f64_*` intrinsics + `lib/math.cyr` helpers.
      Zero raw `*`/`+` on sample/gain/coefficient values.
- [ ] Hand-rolled WAV/RIFF parser (replaces `shravan`); SF2/SFZ parsers ported
      (they're already binary/text parsers — good fit for the manual style).
- [ ] `NidhiError` variants mapped to distinct negative int codes + stderr msgs.
- [ ] `alloc_init()` first in every entry `main`; explicit
      `var ec = main(); syscall(SYS_EXIT, ec);` tail.
- [ ] `cargo build`/`cargo test` succeed as a final Rust sanity check **before**
      freezing, then all Rust source moved to `rust-old/`.

**Manifest & pinning**
- [ ] `cyrius.cyml` with `[package] cyrius = "<pinned version>"`,
      `version = "${file:VERSION}"`, `[build] entry/output`, `[deps] stdlib`
      (DSP-minimal list), git deps only if needed (nidhi likely needs **none** —
      naad/shravan/hisab all get hand-ported or replaced by `lib/math.cyr`).
- [ ] `VERSION` unchanged as source of truth; bump to a **major** version (the
      language change is a breaking change — vidya went 1.5→2.0).
- [ ] `cyrius.lock`, `lib/` added to `.gitignore` (build artifacts).

**Tests & benchmarks (the parity story)**
- [ ] `tests/nidhi.tcyr` — cyrius-native tests via `lib/assert.cyr`
      (`assert`/`assert_eq`/`test_group`/`assert_summary`).
- [ ] `tests/nidhi.bcyr` — cyrius-native benches via `lib/bench.cyr`, one
      `bench_*` fn per Rust criterion bench, matching iteration tiers.
- [ ] `bench-history-rust.csv` — captured ONCE from `rust-old/benches` criterion
      run (`benchmark,mean_ns`). **Do this before freezing rust-old.**
- [ ] `bench-history.csv` — Cyrius history seeded with the first port run.
- [ ] `BENCHMARKS.md` — side-by-side Cyrius-vs-Rust ratio table + honest
      per-benchmark takeaways + micro/meso/macro tiering.
- [ ] `scripts/bench-history.sh` calling `cyrius bench` (not `cargo bench`).

**rust-old/ freeze**
- [ ] Entire Rust crate moved to `rust-old/` (Cargo.toml, src, benches, tests,
      examples), untouched.
- [ ] `rust-old/LINES_OF_RUST.txt` = `7180` (or the exact final `wc -l` total).

**CI / release**
- [ ] `.github/workflows/ci.yml` rewritten to the Cyrius shape (§7): install
      pinned toolchain from `cyrius.cyml`, `cyrius deps`, per-file lint, build,
      ELF verify, smoke, `cyrius test`, `cyrius bench`, docs/version checks.
- [ ] `.github/workflows/release.yml` rewritten: CI gate → version==tag verify →
      `cyrius build` (x86_64 + best-effort aarch64) → archive + SHA256SUMS →
      gh-release with awk-extracted CHANGELOG section.

**Docs**
- [ ] `docs/adr/0001-port-from-rust-to-cyrius.md` — the decision record (context,
      decision, scope in/out, consequences incl. perf asymmetries).
- [ ] `docs/development/state.md` — version, cyrius pin + rationale, binary size,
      dep pins, stdlib module list.
- [ ] `CLAUDE.md` rewritten for the cyrius toolchain (no cargo/clippy/audit/deny;
      `cyrius build/test/bench/lint/fmt` instead; note "no longer a Rust crate").
- [ ] `CHANGELOG.md` — one big `[<major>.0.0]` port entry with a **Breaking**
      section, Cyrius-vs-Rust perf table, and LOC/binary-size deltas.
- [ ] `README.md` updated (build with cyrius, not cargo).
- [ ] `scripts/version-bump.sh` (writes VERSION + stamps CHANGELOG; no cyml edit).

**Green-light gates (mirror vidya's release.yml)**
- [ ] `cyrius build` produces a valid static ELF (`7f45 4c46` magic).
- [ ] `cyrius test` passes; `cyrius bench` runs.
- [ ] `cyrius lint src/nidhi.cyr` clean except the tolerated 120-char cosmetic.
- [ ] `VERSION` == git tag == appears in CHANGELOG.
