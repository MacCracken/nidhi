# Cyrius STDLIB Catalog for the nidhi Port

Read-only reconnaissance brief. Everything here was extracted from the actual
vendored `.cyr` sources plus the compiler frontend, not from memory. Signatures
are copied verbatim from source (`grep "^fn "`). This is written so another
engineer can produce correct Cyrius without re-reading the sources.

## 0. Cyrius mental model (only the parts that bite an audio port)

- **Everything is `i64`.** There are no other scalar types at runtime. Function
  signatures are written `fn name(args): i64 { ... }`. Typed params exist as
  *hints* only: `s: cstring`, `s: Str`. Return type is nearly always `: i64`.
- **Floats are f64 bit-patterns stored in i64 slots.** A `1.0` is the literal
  `0x3FF0000000000000`. You never store a "float" — you store its 64-bit IEEE
  pattern in an i64 register/slot and operate on it with the f64 intrinsics
  below. An audio sample buffer is therefore an array of i64 slots each holding
  an f64 bit-pattern.
- **Heap structs are raw byte blocks.** You `alloc(n)` a block and read/write
  fields with `load64(ptr + off)` / `store64(ptr + off, val)`. Field access is
  manual offset arithmetic. Widths: `load8/16/32/64`, `store8/16/32/64` (these
  are compiler builtins, not stdlib).
- **`#derive(accessors)`** (v5.9.9) on a `struct` decl auto-emits
  `Type_field(self)` getters and `Type_set_field(self, v)` setters at compile
  time (confirmed in `programs/cyrius_api_surface.cyr:209-324`). Without it you
  write the `load64/store64` by hand. The stdlib itself mostly does it by hand
  (e.g. `fn map_cap(m): i64 { return load64(m + 8); }`).
- **Errors are negative integers.** Conventions in the stdlib: return `0` for
  "not found / empty / OOM" in the legacy API; the newer `_r` variants return a
  `Result` (tagged.cyr / result.cyr) and pair with the `?` operator. IO errno is
  mapped to small negative codes.
- **No serde, no generics, no trait objects, no closures.** `fnptr.cyr` gives
  raw indirect calls (`fncall0..8`). Iteration callbacks pass a function
  pointer + call it via `fncall*`.

### How to include a module

```
include "lib/alloc.cyr"
include "lib/vec.cyr"
```

Paths are **relative to the project/compiler root** and always start with
`lib/`. Order matters: a module must be included *after* its dependencies.
`alloc_init()` must be called once at program start before any allocation.
Canonical prelude for nidhi (superset, in dependency order):

```
include "lib/syscalls.cyr"   # arch-selected syscall wrappers (pulled by io/alloc)
include "lib/string.cyr"     # strlen/memcpy/memset/memeq + print
include "lib/alloc.cyr"      # heap; call alloc_init() first
include "lib/fmt.cyr"        # int/hex/float formatting
include "lib/math.cyr"       # f64 helpers + constants + f64_parse
include "lib/str.cyr"        # fat-string Str + builder + split/trim
include "lib/vec.cyr"        # dynamic i64 array (sample buffers)
include "lib/io.cyr"         # file open/read/write, read-whole-file
include "lib/tagged.cyr"     # Option/Result (transitively includes result.cyr)
```

Then for the sample bank and (optionally) transcendental math:

```
include "lib/hashmap.cyr"    # map_u64_* — integer-keyed sample bank  (vidya)
include "lib/ganita.cyr"     # f64_pow/sqrt-family + linalg           (vidya/hisab)
```

> **Source-of-truth note.** Two vendored stdlib trees exist on this machine:
> `/home/macro/Repos/hisab/lib/` (leaner) and `/home/macro/Repos/vidya/lib/`
> (superset). **`hashmap.cyr`, `fs.cyr`, `regex.cyr`, `bayan.cyr`, `slice.cyr`
> exist ONLY in vidya's tree**, not hisab's. The core modules (vec/str/io/fmt/
> math/tagged/fnptr/args/alloc/syscalls) are byte-identical between the two.
> Use vidya's tree as the reference for the extra modules.

---

## 1. PRIORITY MODULES FOR NIDHI

### 1a. `lib/vec.cyr` — dynamic array = audio sample buffer

`include "lib/alloc.cyr"` then `include "lib/vec.cyr"`. Requires `alloc_init()`.

**Layout (24-byte heap struct):** `{ data_ptr@0, len@8, cap@16 }`. Elements are
**8 bytes (i64) each**. Initial cap = 16, grows 2x. The first 16 bytes are
byte-identical to a `slice`/`Str` (`data@0, len@8`) so a vec passes directly to
slice helpers.

**This is THE audio buffer type: store each f64 sample as its bit-pattern in an
i64 slot.** `vec_push(buf, 0x3FF0000000000000)` pushes `1.0`.

| Signature | Semantics |
|-----------|-----------|
| `fn vec_new(): i64` | New empty vec, cap 16. Returns vec ptr (0 on OOM in `_a` form). |
| `fn vec_new_a(a): i64` | Same, via explicit Allocator `a`. |
| `fn vec_len(v): i64` | `load64(v+8)` — element count. |
| `fn vec_cap(v): i64` | `load64(v+16)` — capacity. |
| `fn vec_get(v, idx): i64` | Bounds-checked read (aborts process on OOB). |
| `fn vec_set(v, idx, val): i64` | Bounds-checked write (aborts on OOB). |
| `fn vec_push(v, val): i64` | Append, auto-grow 2x. Aborts on OOM. |
| `fn vec_push_a(a, v, val): i64` | Append via allocator; returns `-1` on OOM (graceful). |
| `fn vec_pop(v): i64` | Remove+return last; `0` if empty. |
| `fn vec_truncate(v, new_len): i64` | Shrink len, keep cap/data (reset without free). |
| `fn vec_find(v, val): i64` | Linear search, index or `-1`. |
| `fn vec_remove(v, idx): i64` | Remove+shift-left; `-1` if OOB. |

**Gaps nidhi must implement itself:**
- No `vec_free` — the bump allocator does not free individual blocks; use
  `vec_truncate` for reuse or `alloc_reset()` for a batch wipe. **A frame-hot
  render loop must NOT `vec_new` per callback** (leaks under bump alloc).
- No typed/f32 vec — elements are always 8 bytes. If you want compact f32 sample
  storage, allocate a raw byte block via `alloc(n*4)` and use `store32/load32`
  with `f32_from`/`f32_to` (builtins) yourself; vec won't do it.
- No `vec_reserve`, no `vec_extend`, no bulk copy — write your own over `load64`/
  `store64` on `vec_data = load64(v)`.

### 1b. `lib/hashmap.cyr` (vidya only) — sample bank keyed by integer id

`include "lib/hashmap.cyr"` (after alloc.cyr, string.cyr). Open-addressing,
linear-probe, tombstones, 2x grow. **Layout `{ entries@0, cap@8, count@16,
keytype@24 }`; entries array is `cap * 24` bytes, each entry
`{ key@0, value@8, state@16 }` (state: 0 empty, 1 occupied, 2 tomb).**

**Use the `map_u64_*` family for nidhi's sample bank** (integer sample-id →
sample-struct-ptr). The generic `map_*` family is cstr/Str-keyed and is the
wrong tool for integer keys.

| Signature | Semantics |
|-----------|-----------|
| `fn map_u64_new(): i64` | New integer-keyed map, cap 16. Returns map ptr. |
| `fn map_u64_new_a(a): i64` | Same via allocator. |
| `fn map_u64_set(m, key, value): i64` | Insert/overwrite. `0` ok, `<0` on OOM. |
| `fn map_u64_get(m, key): i64` | Value, or **`0` if absent** (ambiguous — see note). |
| `fn map_u64_get_or(m, key, default_val): i64` | Value, or `default_val` if absent. |
| `fn map_u64_has(m, key): i64` | `1` present / `0` absent. |
| `fn map_u64_delete(m, key): i64` | `1` if removed / `0` if absent (tombstones). |
| `fn map_u64_size(m): i64` | `load64(m+16)` — live entry count. |
| `fn map_u64_clear(m): i64` | Wipe entries, keep capacity. |

**Note:** `map_u64_get` returns `0` for a missing key AND for a key whose stored
value is `0`. Since your sample values are heap pointers (never 0 for a live
sample), that's usually safe, but prefer `map_u64_has` + `map_u64_get`, or
`map_u64_get_or(m, id, -1)`, when 0 is a legal value.

**Generic key families (for the SFZ opcode tables, string-keyed):**
`fn map_new(): i64` (cstr keys), `fn map_new_str(): i64` (Str-struct keys — use
this one if keys come from `str_from_int`/`str_new`; the cstr map mis-hashes Str
structs and silently drops ~3% of entries). API mirrors u64:
`map_set/map_get/map_get_or/map_has/map_delete/map_size/map_keys/map_values/
map_clear`, plus `fn map_iter(m, fp): i64` (calls fnptr `fp` per entry).
`hashmap_fast.cyr` (`fhm_*`) is a SwissTable-style faster variant with the same
shape if profiling demands it.

**Gap:** no generic "map of anything" — you pick a key family. No serialization.

### 1c. `lib/str.cyr` — fat string `Str` for the SFZ text parser

`include` after alloc.cyr + string.cyr. **`Str` = 16-byte fat pointer
`{ data@0, len@8 }`** (shares layout with slice/vec prefix). Not
null-terminated; carries an explicit length. Dot-access `s.data` / `s.len` works
on `: Str`-typed fn-locals (v5.8.17).

**Construction / inspection:**

| Signature | Semantics |
|-----------|-----------|
| `fn str_from(cstr): Str` | Wrap a NUL-terminated C string (shares bytes). |
| `fn str_new(data, len): Str` | From (ptr,len) buffer. |
| `fn str_from_buf(ptr, len): Str` | Alias of str_new. |
| `fn str_len(s: Str): i64` / `fn str_data(s: Str): i64` | Field reads. |
| `fn str_eq(a: Str, b: Str): i64` | Content equality → 0/1. |
| `fn str_eq_cstr(s, cstr): i64` | Compare Str to C string. |
| `fn str_clone(s: Str): Str` | Deep copy (new alloc). |
| `fn str_cstr(s): i64` | Materialize NUL-terminated copy → char* (for syscalls). |

**Parsing helpers the SFZ parser needs:**

| Signature | Semantics |
|-----------|-----------|
| `fn str_trim(s: Str): Str` | Strip leading/trailing ASCII whitespace (shares data). |
| `fn str_split(s: Str, sep): i64` | Split by **byte** (`sep < 256`) OR by **Str** separator; returns a **vec of Str** (each is a heap `Str` fat-ptr). |
| `fn str_split_cstr(s, sep_cstr): i64` | Split by a C-string separator. |
| `fn str_starts_with(s: Str, prefix: Str): i64` | 0/1. |
| `fn str_ends_with(s: Str, suffix: Str): i64` | 0/1. |
| `fn str_contains(s: Str, needle: Str): i64` | 0/1 substring search. |
| `fn str_contains_cstr(s, needle_cstr): i64` | 0/1. |
| `fn str_index_of(s: Str, ch): i64` | Byte index or `-1`. |
| `fn str_index_of_cstr(s, needle_cstr): i64` | Substring index or `-1`. |
| `fn str_last_index_of(s, needle_cstr): i64` | Last occurrence or `-1`. |
| `fn str_sub(s: Str, start, len): Str` | Substring by (start,len), **shares data** (no copy). |
| `fn str_substr(s: Str, start, end): Str` | Substring by (start,end). |
| `fn str_to_int(s: Str): i64` | Parse decimal integer (also `atoi(cstr)` in string.cyr). |
| `fn str_from_int(n): Str` | Integer → Str. |

**String building (for emitting/normalizing text):** builder layout
`{ buf@0, len@8, cap@16 }`, 64-byte inline buffer growing via alloc.

| Signature | Semantics |
|-----------|-----------|
| `fn str_builder_new(): i64` | New builder. |
| `fn str_builder_add(sb, s: Str): i64` | Append a Str. |
| `fn str_builder_add_cstr(sb, cstr): i64` | Append a C string. |
| `fn str_builder_add_int(sb, n): i64` | Append decimal integer. |
| `fn str_builder_add_byte(sb, byte): i64` / `str_builder_putc(sb, ch)` | Append one byte. |
| `fn str_builder_build(sb): i64` | Finalize → `Str`. |
| `fn str_join(parts, sep: Str): Str` | Join a vec of Str with separator. |

**Float parsing for SFZ opcode values** lives in `math.cyr`, not str.cyr:
`fn f64_parse(s): i64` — parse a NUL-terminated string to an f64 bit-pattern
(handles sign, `.`, exponent, `nan`/`inf`; returns `0` = `0.0` on no-digits, so
use `f64_parse_ok(s, out): i64` which returns a success flag when you must
distinguish `"0"` from garbage). **`f64_parse` takes a C string (char*), not a
`Str`** — call `str_cstr(str_val)` first, or ensure NUL-termination.

**Gap:** `str_to_int` has no error flag (returns 0 on garbage); write your own
validating parser for opcodes that must reject bad input.

### 1d. Byte / binary reading — for SF2 and WAV parsers

**There is no `bytes`/`reader` module.** You read a file into a raw buffer
(`file_read_all`, below) and pull integers out with the compiler builtins:

- `load8(p)`, `load16(p)`, `load32(p)`, `load64(p)` — read 1/2/4/8 bytes at
  address `p`. **These are little-endian on x86_64/aarch64** (the native byte
  order and what the compiler emits), which is exactly what RIFF/WAV and SF2 use
  — so `load16(buf+off)` directly gives you an LE `u16`, `load32(buf+off)` an LE
  `u32`. No byte-swap needed on LE targets.
- `store8/16/32/64(p, v)` — the write side.
- **Bounds-checked variants** (from the compiler / reference): `checked_load8`,
  `checked_load64`, `checked_store8`, `checked_store64` take `(buf, len, idx)`
  and trap on OOB — use these when parsing untrusted SF2/WAV headers.
- Sub-word values are unsigned-zero-extended by `load8/16/32`. For **signed
  16-bit PCM** (WAV `s16`), sign-extend yourself:
  `var s = load16(p); if (s >= 0x8000) { s = s - 0x10000; }`.
- **f32 sample data → f64**: read the 32 raw bits with `load32`, then
  `f64 = f32_to(bits)` (builtin, token 132, `cvtss2sd`). Reverse with
  `f32_from(f64bits)` (token 131). For **s16 → f64 normalized**:
  `f64_div(f64_from(sample_i16), f64_from(32768))`.

**Gaps nidhi must implement itself:** big-endian reads (AIFF is BE — swap bytes
manually, or read byte-by-byte), 24-bit PCM unpacking, and any "cursor" struct
(track a `pos` i64 alongside the buffer). All trivial with `load8` and shifts.
`bayan.cyr` has base64 and a `_u128`/`_u256` bigint (for GUIDs/chunk sizes if
ever needed) but nothing WAV/SF2-specific.

### 1e. `lib/io.cyr` — file IO (open / read / close / read-whole-file)

`include "lib/io.cyr"` (after syscalls.cyr). Flag constants defined here:
`O_RDONLY=0, O_WRONLY=1, O_RDWR=2, O_CREAT=64, O_TRUNC=512, O_APPEND=1024,
O_EXCL=128`; `STDIN=0, STDOUT=1, STDERR=2`.

| Signature | Semantics |
|-----------|-----------|
| `fn file_open(path: cstring, flags, mode): i64` | Open, returns fd (or negative errno). |
| `fn file_close(fd): i64` | Close. |
| `fn file_read(fd, buf, len): i64` | Read up to `len` bytes → count. |
| `fn file_write(fd, buf, len): i64` | Write. |
| `fn file_read_all(path, buf, maxlen): i64` | **Read whole file into `buf` (cap `maxlen`) → bytes read.** Primary entry for WAV/SF2/SFZ loading. |
| `fn file_write_all(path, buf, len): i64` | Write whole buffer. |
| `fn file_exists(path): i64` | 0/1. |
| `fn xlseek(fd, off, whence): i64` | Seek (for streaming large samples). |
| `fn getenv(name): i64` | Env var lookup → char* / 0. |

**Result variants (v5.8.30), pair with `?`:** `file_open_r`, `file_close_r`,
`file_read_r`, `file_write_r`, `file_read_all_r`, `file_write_all_r` each return
a `Result<T, IoError>` where `IoError ∈ {IoNotFound, IoAccessDenied, IoBadFd,
IoFailed, IoOther}`. Example: `var fd = file_open_r(path, 0, 0)?;`

**`path` args are C strings (char*)** — `str_cstr(s)` a `Str` first. **`buf`
must be pre-allocated** (`alloc(maxlen)`); `file_read_all` does NOT allocate for
you and does NOT grow — size `maxlen` from the file size or pick a ceiling and
loop with `file_read` for streaming. No mmap-file helper in io.cyr
(`mmap.cyr`/`lib/mmap.cyr` exists in vidya if you want zero-copy sample
streaming later).

### 1f. f64 math — sin/cos/pow/sqrt/floor/abs/exp/log

**Two layers. Know which is which — it changes whether you `include` anything.**

**(A) Compiler builtins (NO include, always available).** These are lexer
keywords compiled to inline SSE2/x87 (x86) or FP instrs / polyfill-calls
(aarch64). Confirmed in `src/frontend/parse_expr.cyr:1958-2000`. Each takes/
returns an f64 bit-pattern in i64 unless noted:

| Builtin | Op |
|---------|----|
| `f64_add(a,b)` `f64_sub(a,b)` `f64_mul(a,b)` `f64_div(a,b)` | Arithmetic (ptyp 62-65). |
| `f64_from(int): f64` | **int → f64** (SCVTF/cvtsi2sd). Your `1.0` = `f64_from(1)`. (ptyp 66) |
| `f64_to(f64): int` | **f64 → int**, truncating toward zero (ptyp 67). *(reference doc also calls the int-cast `f64_to`; there is no separate `f64_to_int` builtin.)* |
| `f64_lt(a,b)` `f64_gt(a,b)` `f64_eq(a,b)` | Compare → 0/1 (ptyp 68-70). NaN → 0. |
| `f64_neg(x)` | Negate (ptyp 71). |
| `f64_sqrt(x)` | Square root (ptyp 79, `sqrtsd`). |
| `f64_abs(x)` | Absolute value (ptyp 80). |
| `f64_floor(x)` `f64_ceil(x)` `f64_round(x)` | Rounding (ptyp 81/82/92, SSE4.1 `roundsd`). |
| `f64_sin(x)` `f64_cos(x)` | Trig (ptyp 83/84; x87 `fsin`/`fcos` on x86). |
| `f64_exp(x)` `f64_ln(x)` `f64_log2(x)` `f64_exp2(x)` | exp/log family (ptyp 85-88; polyfilled on aarch64). |
| `f64_atan(x)` | Arctangent (ptyp 99). |
| `f32_from(f64bits): u32bits` / `f32_to(u32bits): f64bits` | f64↔f32 conversion (ptyp 131/132). |

**No `f64_tan` builtin** (compute `f64_div(f64_sin(x), f64_cos(x))`). **No
`f64_pow` builtin** — it's in the stdlib layer below.

**(B) `lib/math.cyr` (stdlib helpers over the builtins).** `include
"lib/math.cyr"` — its header says "uses f64 builtins directly, no other
includes." Provides constants and helpers:

- **Constants (f64 bit-patterns, as `var`):** `F64_ONE, F64_TWO, F64_HALF, F64_PI,
  F64_PI_2, F64_PI_4, F64_TAU, F64_E, F64_LN2, F64_LN10, F64_LOG2E, F64_SQRT2,
  F64_FRAC_1_SQRT2`, etc. Use these instead of hardcoding hex.
- **Helpers:** `fn f64_clamp(x, lo, hi)`, `fn f64_min(a,b)`, `fn f64_max(a,b)`,
  `fn f64_le(a,b)`, `fn f64_ge(a,b)` (NaN-safe `<=`/`>=`, since builtins only give
  `<`/`>`/`==`), `fn f64_lerp(a, b, t)` (crossfade/gain ramps), `fn f64_sign(x)`,
  `fn f64_trunc(x)`, `fn f64_fract(x)`, `fn f64_hypot(x, y)`, `fn gcd(a,b)`,
  `fn lcm(a,b)`, `fn f64_parse(s)`, `fn f64_parse_ok(s, out)`.

**(C) `lib/ganita.cyr` (vidya/hisab) — transcendental + power.** `include
"lib/ganita.cyr"` (keep math.cyr in scope for the exp/ln polyfills). Provides:
`fn f64_pow(base, exp): i64`, `fn f64_sinh/cosh/tanh(x)`, `fn f64_asin/acos(x)`
(x86 only), `fn f64_atan2(y, x)` (x86 only), `fn f64_asinh/acosh/atanh(x)`,
`fn f64_hypot(x,y)`. (These are thin aliases to `ganita_f64_*`.) **`f64_pow`
lives here, not in math.cyr** — pitch-shift ratio `2^(semitones/12)` =
`f64_pow(F64_TWO, f64_div(f64_from(semitones), f64_from(12)))`.

**Platform caveat:** `f64_asin/acos/atan2` are **x86-only** (aarch64 hard-errors
or lacks them). If nidhi must run on aarch64/agnos, avoid those three or provide
your own polyfill. `f64_exp/f64_ln` ARE polyfilled on aarch64 (safe everywhere).

---

## 2. OTHER CORE MODULES (brief)

### `lib/syscalls.cyr` (+ arch peers) — raw syscalls
`include "lib/syscalls.cyr"` selects the arch peer (`syscalls_x86_64_linux.cyr`,
`syscalls_aarch64_linux.cyr`, `syscalls_x86_64_agnos.cyr`, macos, windows). The
`syscall(n, ...)` form is a compiler builtin. Defines `O_*`, `SEEK_*`, `mmap`/
`munmap`, `sys_read`/`sys_write`. nidhi rarely calls these directly — io.cyr/
alloc.cyr wrap them.

### `lib/alloc.cyr` — heap
`include "lib/alloc.cyr"`. **Bump allocator over mmap chunks.** Call
`alloc_init()` once (idempotent) before anything. `fn alloc(size): i64`
(8-byte-aligned ptr), `fn alloc_reset(): i64` (frees EVERYTHING at once),
`fn alloc_used(): i64`. **No per-object free.** Also offers **arenas**
(`arena_new(cap)`, `arena_alloc(a,size)`, `arena_reset(a)`, `arena_free(a)`) and
a pluggable Allocator vtable (`default_alloc()`, `bump_allocator()`,
`arena_allocator(cap)`) that the `*_a` stdlib variants accept. **Design
implication for nidhi:** allocate sample buffers up front; use an arena per
loaded instrument so unloading = one `arena_reset`; never allocate in the render
callback.

### `lib/string.cyr` — C-string / memory ops
`fn strlen(s)`, `fn streq(a,b)`, `fn memeq(a,b,n)`, `fn memcpy(dst,src,n)`,
`fn memset(dst,val,n)` (use for zeroing sample buffers), `fn memchr(s,c,n)`,
`fn strchr(s,c)`, `fn strstr(haystack,needle)`, `fn atoi(s)`, `fn println(s)`,
`fn print_num(n)`, plus in-place case fold.

### `lib/fmt.cyr` — formatting
`fn fmt_int(n)`, `fn fmt_hex(n)`, `fn fmt_hex0x(n)`, `fn fmt_int_buf(n, buf)`,
`fn fmt_float(val, decimals)` and `fn fmt_float_buf(val, buf, decimals)`
(**takes an f64 bit-pattern** — for logging sample values / gains),
`fn fmt_sprintf(buf, bufsz, fmt, args)` and `fn fmt_printf(fmt, args)`
(`%d %x %s %%`).

### `lib/tagged.cyr` / `lib/result.cyr` — Option / Result
`include "lib/tagged.cyr"` (transitively includes result.cyr). `fn tagged_new(tag,value)`,
`fn tag(t)`, `fn payload(t)`, `fn is_tag(t,expected)`, `fn is_some/is_none(opt)`,
`fn unwrap(opt)`, `fn unwrap_or(opt, fallback)`, `None()`. Pairs with the `?`
operator on `Result`-returning fns (the io `_r` variants). Use these for the
public nidhi API surface instead of magic sentinels.

### `lib/fnptr.cyr` — indirect calls (callbacks / round-robin dispatch)
`fn fncall0(fp)` … `fn fncall8(fp, a,b,c,d,e,f,g,h)`. This is how you pass a
render/voice callback or iterate a map (`map_iter(m, fp)`). No closures — bundle
state in a struct ptr passed as an argument.

### `lib/args.cyr` — CLI args
`fn args_init()` (call once), `fn argc(): i64`, `fn argv(n): i64` (→ char*).
Relevant only for a nidhi CLI/demo, not the library core.

### `lib/slice.cyr` (vidya) — 16-byte fat pointer `{ptr@0, len@8}`
Zero-copy views over a buffer/vec. `fn slice_from_buf(dst, buf, len)`,
`fn vec_as_slice(dst, v)`, `fn slice_ptr/slice_len`, sized loads
`_slice_idx_get_1/2/4/8/16` and `slice_unchecked_get_*`. Useful for passing a
window of a sample buffer without copying. `s[i]`/`s.ptr` sugar works on
fn-locals.

### `lib/fs.cyr` (vidya) — path + directory helpers
`fn path_join(dir,name)`, `fn path_basename(path)`, `fn path_dirname(path)`,
`fn path_has_ext(path, ext)`, `fn dir_list(path)`, `fn is_dir(path)`,
`fn find_files(path, ext)`, `fn dir_walk(path, results)`. Good for locating
`.wav`/`.sfz`/`.sf2` files referenced by an SFZ `sample=` opcode (resolve
relative paths against the SFZ dir with `path_dirname` + `path_join`).

### `lib/regex.cyr` (vidya) — lightweight regex + string replace
`fn glob_match(pattern, text)`, `fn str_glob(s, pattern)`,
`fn find_all(haystack, needle)`, `fn str_replace(s, old, new)`,
`fn str_replace_all(s, old, new)`, plus a full Pike-VM
(`regex_compile(pat)` → `regex_match(nfa, s)` / `regex_search` /
`regex_group_start/end`). Probably overkill for SFZ; `str_split`/`str_trim` +
manual scanning is simpler. (The heavier `niyama` engine exists too but is not
needed.)

### `lib/bayan.cyr` (vidya) — data formats + bigint (opt-in)
`fn bayan_json_parse(src)`, `bayan_json_get/get_int`, `bayan_toml_parse`,
`bayan_base64_encode/decode`, `bayan_csv_parse_line`, and `u128`/`u256` bigint.
**Not needed for WAV/SF2/SFZ** (those are binary/ini-like, not JSON/TOML). Pull
it only if nidhi grows a JSON preset format. base64 is here if a preset ever
embeds sample data.

---

## 3. WHAT NIDHI MUST IMPLEMENT ITSELF (no stdlib equivalent)

1. **f32 / s16 / s24 sample-buffer storage.** vec is i64-only (8 B/elem).
   Either waste 8 B/sample storing f64 bit-patterns in a vec, or hand-roll a raw
   `alloc(n*4)` block with `store32`/`load32` + `f32_from`/`f32_to`. No typed
   numeric array exists.
2. **Big-endian binary reads (AIFF).** `load16/32` are little-endian only;
   byte-swap manually. WAV/SF2 are LE so those are fine as-is.
3. **Signed / 24-bit PCM unpacking.** `load16` zero-extends; sign-extend and
   assemble 24-bit yourself.
4. **A binary "cursor"/reader struct.** Track `pos` alongside the buffer ptr;
   trivial but not provided.
5. **`f64_tan`, `f64_log10`.** Not builtins — derive from `sin/cos` and
   `f64_ln`/`F64_LN10`.
6. **Any DSP** — filters, envelopes (ADSR), LFOs, WSOLA/OLA time-stretch,
   interpolation, crossfades. The stdlib has zero audio-DSP. `lib/vani.cyr`
   (vidya) is ALSA PCM output + a ring buffer + a basic mixer — an *output
   sink*, not a sample engine — nidhi is the engine feeding something like it.
7. **Serialization / round-trip.** No serde. Every "must be Serialize" type from
   the Rust side becomes a hand-written `type_to_bytes` / `type_from_bytes`
   pair, or a `#derive(Serialize)`-style accessor emit if the compiler supports
   it (it emits accessors via `#derive(accessors)`; a `#derive(Serialize)` token
   is recognized by the api-surface tool but verify codegen before relying on
   it).
8. **Per-object free / GC.** Bump/arena only. Architect lifetimes around
   `arena_reset` boundaries (per-instrument, per-load).

---

## 4. Key file paths (all absolute)

- Docs: `/home/macro/Repos/cyrius/docs/stdlib-reference.md`,
  `/home/macro/Repos/cyrius/docs/stdlib-modules.md`
- f64/load/store builtin dispatch (authoritative):
  `/home/macro/Repos/cyrius/src/frontend/parse_expr.cyr:1958-2000` (f64 ptyps),
  `:1287-1353` (load8/16/32/64, store8/16/32/64)
- Core stdlib (byte-identical in both trees):
  `/home/macro/Repos/hisab/lib/{vec,str,string,io,fmt,math,tagged,fnptr,args,alloc,syscalls}.cyr`
- Vidya-only modules:
  `/home/macro/Repos/vidya/lib/{hashmap,hashmap_fast,fs,slice,regex,bayan,ganita,mmap,vani}.cyr`
- Transcendental/power: `/home/macro/Repos/hisab/lib/ganita.cyr` (also in vidya)
- `#derive(accessors)` reference: `/home/macro/Repos/cyrius/programs/cyrius_api_surface.cyr:209-324`
