# Cyrius Language Reference — for the nidhi (Rust → Cyrius) Port

> Read-only reconnaissance brief. Sources: `cyrius/docs/guides/cyrius-guide.md`,
> `tutorial.md`, `faq.md`, `docs/architecture/cyrius.md`, ADR-002/003/004,
> `cyrius/lib/{math,tagged,result,str,trait,alloc,vec,fnptr,fmt}.cyr`, and
> `vidya/content/cyrius/language/*.cyml`. Versions cited are current at Cyrius
> **v6.2.x / v6.3.x**. When this brief and the compiler disagree, the compiler
> and `src/main.cyr`'s HEAP MAP win.

---

## 0. The one mental model you must internalize

**Everything is an i64 (ADR-002).** There is no bool, char, pointer, or (for
most code) float type at the machine level. Every value is a 64-bit integer in a
register or an 8-byte slot. Type annotations (`: i64`, `: Str`, `: f64`) are
*documentation plus width/codegen hints* — they do **not** create a checked type
system for structs/pointers. Consequences that shape the entire port:

- A pointer is just an i64 address. `&x`, `p + 8`, `load64(p)` are integer math.
- A struct is contiguous i64 (or width-annotated) fields at fixed byte offsets.
  There are no methods, no vtables, no generics on structs in the Rust sense.
- Errors are **negative integer codes** or a heap-allocated tagged `Result`.
- Floats (`f64`) are **IEEE-754 bit patterns stored in i64**, manipulated by
  `f64_add`/`f64_mul`/… builtins (or, since v6.2.19, by bare operators on
  `: f64`-annotated vars). `f32` is storage/convert-only — no f32 arithmetic.

There is **no auto-`main()`**. The program is the top-level statements executed
in source order (see §14).

---

## 1. File, comments, entry

- Source files are `.cyr`. Comments start with `#` and run to end of line.
  There is **no** `//` or `/* */`. (`#` also introduces preprocessor directives
  and attributes — context distinguishes them.)
- No `<!DOCTYPE>`-style wrapper; the file *is* the program.
- **No `main()` auto-call.** A `fn main()` is just a function. To run it:

```
fn main() {
    syscall(1, 1, "hi\n", 3);
    return 0;
}
var rc = main();          # top-level statement — THIS is the entry
syscall(60, rc);          # exit(rc)   (Linux SYS_EXIT = 60)
```

Or skip the wrapper entirely: `syscall(60, 42);` is a complete program.

---

## 2. Variables, consts, arrays

```
var x = 42;               # untyped → i64
var y: i64 = 42;          # annotation is documentation (same codegen)
x = x + 1;                # reassignment
```

There is **no `const` keyword and no `let`.** "Constants" are `var` globals or
enum variants (see §7). Globals are declared at top level, visible to functions
above and below them (declaration-order init — a forward reference in a global
initializer silently reads 0; the linter warns).

**Number literals:** decimal `1_000_000`, hex `0x1ED`, octal `0o755`. Underscores
allowed and ignored. **No binary `0b`.** **No negative literals** — write
`(0 - N)`, not `-N`. A literal with a fractional part (`3.14`) lexes as f64.

**Char literals** (v5.9.37): `'A'` is a NUMBER token = 65. Escapes `\n \t \r \\ \' \0`.
Single ASCII byte only.

**Multi-width scalars** (annotation-driven; nidhi needs these for packed sample
data / headers):

```
var b: i8  = 42;      # 1 byte
var h: i16 = 30000;   # 2 bytes
var w: i32 = 100000;  # 4 bytes
var q: i64 = 0;       # 8 bytes (default)
var u: u128 = 0;      # 16 bytes (storage; arithmetic via lib/u128.cyr)
```
Widths: `i8/i16/i32/i64`, `u8..u64`, `u128`. All `iN` are **signed**; `/` and `%`
lower to `idiv` (signed) — for unsigned high-bit math use stdlib `u64_*` helpers.

**Arrays — the byte-vs-slot footgun (critical):**

| Spelling        | Reserved bytes | Meaning |
|-----------------|----------------|---------|
| `var a: i64[N]` | `N * 8`        | **N i64 slots** — same in fn and top level (USE THIS) |
| `var a: i32[N]` | `N * 4`        | packed 32-bit |
| `var a: u8[N]`  | `N`            | byte buffer |
| `var a[N]` (bare, **in a fn**)     | `N` **bytes** | byte buffer only |
| `var a[N]` (bare, **top level**)   | `N*8` (N slots) | scope-dependent — avoid |

Always use the explicit `var a: i64[N]` for slot arrays. A function-local
`var a[4]` is **4 bytes = one slot**, so `store64(&a + i*8, …)` for i>0 runs off
the backing. `var a[N] = { 0x.., ... }` is a fixed byte-array literal (element
count must equal N).

Enum-const idents work as array sizes: `enum Sz { SMALL=256; } var buf[SMALL];`.

> Porting note: for large sample buffers (>~4 KB), do NOT declare `var buf[N]`
> (it writes N zeros into the binary). Use `var buf = 0;` then `buf = alloc(N);`
> at runtime.

---

## 3. Booleans

No `bool` type, no `true`/`false` keywords. Comparisons return the i64 `1` or
`0`. Idiom: `if (is_ok(r) == 1) { … }`. `fmt_bool(b)` prints "true"/"false".
Any nonzero is truthy in practice, but stdlib convention is explicit `== 1`.

---

## 4. Floats — the load-bearing detail for nidhi's DSP

`f64` values are **IEEE-754 bit patterns held in i64**. Two ways to compute:

### (a) Explicit builtins (works everywhere, all versions)

```
var a = f64_from(3);          # int 3 → f64 bit pattern for 3.0
var b = F64_HALF;             # 0.5 as a bit-pattern constant (see below)
var s = f64_add(a, b);        # 3.5
var p = f64_mul(a, b);
var q = f64_div(a, b);
var d = f64_sub(a, b);
var r = f64_sqrt(a);
var n = f64_neg(a);
var v = f64_abs(a);
var fl = f64_floor(a);
var rd = f64_round(a);        # nearest integer, as f64
var i  = f64_to(a);           # f64 → int (truncate/convert back to i64 int)
```

**Comparisons return 1/0** (NaN-safe after the parity-flag fix):
`f64_lt(a,b)`, `f64_gt(a,b)`, `f64_eq(a,b)`. There are **no `<=`/`>=` builtins** —
`lib/math.cyr` provides `f64_le`/`f64_ge`/`f64_clamp`/`f64_min`/`f64_max`.

Transcendentals in `lib/math.cyr`: `f64_exp`, `f64_ln`, `f64_atan` (x87 `fpatan`
on x86), plus composed `sinh/cosh/tanh/pow/hypot/lerp/sign/trunc/fract`. And
`f64_parse` (string → f64). `lib/math.cyr` needs no other include.

**Real f64 bit-pattern constants** (from `lib/math.cyr`, hex = IEEE-754 layout):

```
var F64_ONE  = 0x3FF0000000000000;   # 1.0
var F64_TWO  = 0x4000000000000000;   # 2.0
var F64_HALF = 0x3FE0000000000000;   # 0.5
var F64_PI   = 0x400921FB54442D18;   # π
var F64_TAU  = 0x401921FB54442D18;   # 2π   (nidhi: LFO / phase math)
var F64_E    = 0x4005BF0A8B145769;   # e
var F64_LN2  = 0x3FE62E42FEFA39EF;
var F64_SQRT2= 0x3FF6A09E667F3BCD;
```
To build an arbitrary constant `k`: either `f64_from(k)` at runtime for integers,
or precompute the 64-bit hex pattern. Pattern-pack `2^n` as `((n + 1023) << 52)`.

### (b) Named `: f64` operator sugar (v6.2.19+)

If a var is annotated `: f64`, bare operators lower to float ops:

```
var pi: f64 = F64_PI;
var x:  f64 = f64_from(sample);
var g:  f64 = x * pi + F64_HALF;   # + - * /  → EMIT_F64_BINOP (addsd/mulsd…)
if (x < pi) { … }                  # < > <= >= == !=  → real f64 compare
```
Precedence is normal. Underlying storage is still i64. Prefer explicit
`f64_add(...)` in stdlib-style code for maximum backend/version portability;
the operator sugar is convenience.

### (c) f32 (v6.2.18) — CONVERSION ONLY

`f32_from(d)` (f64→f32 bit pattern) and `f32_to(s)` (f32→f64). **No f32
arithmetic** — widen to f64, compute, narrow back. For 32-bit PCM samples store
as `i32` and widen through `f32_to` / `f64_from` before DSP.

### (d) SIMD packed-double (optional hot-path, `lib/simd.cyr` / builtins)

Flat-array builtins over **raw pointers** and **element counts in doubles**:
`f64v_add/sub/mul/div(dst,a,b,n)`, `f64v_sqrt/abs(dst,src,n)`,
`f64v_fmadd(dst,a,b,c,n)`, `f64v_dot(a,b,n)→f64`, `f64v_scale(dst,a,scalar,n)`,
`f64v_axpy(y,x,alpha,n)`. **Footgun:** odd `n` touches one element past the array
(16-byte lane) — over-allocate by one slot or pass even `n`. Value-type `f64v2`/
`f64v4` params exist on SysV/AAPCS but **not on Windows PE** (use `_ptr` forms).

---

## 5. Strings

Two representations:

1. **C-strings**: a string literal `"hello\n"` is a pointer (i64) to
   NUL-terminated bytes in the data section. `strlen`, `streq`, `strchr`,
   `println` (from `lib/string.cyr`) operate on these. UTF-8 bytes pass through
   verbatim. Escapes: `\n \r \t \0 \\ \" \' \a \b \f \v`, `\x##` (2 hex),
   `\u####` (4 hex), `\u{1..6 hex}` (up to U+10FFFF).

2. **`Str` fat pointer** (`lib/str.cyr`): a **16-byte heap struct
   `{ data; len; }`** (`data` at +0, `len` at +8). Byte-identical to a `slice<u8>`
   and to a vec's first 16 bytes.

```
struct Str { data; len; }
var s: Str = str_from("hello");    # borrows the literal's bytes, heap header
var n = s.len;                     # 5  (pointer-to-struct dot auto-derefs)
var d = s.data;                    # pointer to bytes
str_print(s);  str_println(s);     # NOT println(s) — that prints header garbage
```
Key `Str` fns: `str_from(cstr)`, `str_new(data,len)`, `str_len(s)`,
`str_data(s)`, `str_eq`, `str_cat`, `str_sub`, `str_contains`, `str_split`,
`str_trim`, and a `str_builder_*` family (`str_builder_new/putc/add/add_int/build`).

There is **no `String`/`&str` distinction, no ownership on strings** — `Str`
headers live until the bump allocator resets.

---

## 6. Control flow

```
if (x == 1) { … } elif (x == 2) { … } else { … }

while (cond) { … }

for (var i = 0; i < n; i = i + 1) { … }   # ALL 3 clauses required & non-empty
                                          # step must be a simple assignment
break;      continue;                      # both work in while and for
```
No `for(;;)`; use `while (1 == 1) { … }` for unbounded. **No `++`/`--`**; use
`i += 1` or `i = i + 1`. Compound assignment ops: `+= -= *= /= %= &= |= ^= <<= >>=`.

Operators: arithmetic `+ - * / %`; comparison `== != < > <= >=` (return 1/0);
bitwise `& | ^ ~ << >>`; logical `&& ||` (short-circuit). **Gotcha:** mixed
`&&`/`||` in one condition needs explicit parens — `a && b || c` fails to parse;
write `(a && b) || c`.

Overflow-explicit ops (`lib/overflow.cyr`): `+% -% *%` wrapping (identical bytes
to bare ops), `+| -| *|` saturating, `+? -? *?` checked (panic → `syscall(60,57)`).

**switch** (integer literal cases, no fallthrough, each case independent; bodies
may be blocks with scoped vars):

```
switch (cmd) {
    case 0: return 0;
    case 1: { var buf = alloc(1024); process(buf); }
    default: { result = 0; }
}
```

**match** (v5.8.22+, mainly for enums / tags — see §7/§8):

```
match s {
    PENDING => { r = 1; }
    ACTIVE  => { r = 2; }
    _       => { r = 0; }     # catch-all; without full coverage → compiler warning
}
```
`match` compares against the arm value with a `cmp`/jcc cascade (first match
wins). On a *tagged* value it compares the heap pointer (always unequal) — you
must `match load64(ptr) { … }` to match on the tag, or use helper fns.

---

## 7. Enums (plain integer constants)

```
enum Color { RED; GREEN; BLUE; }              # RED=0, GREEN=1, BLUE=2
enum Err   { OK = 0; NOT_FOUND = 44; PERM = 13; }   # explicit values
var c  = BLUE;          # 2  (bare name)
var c2 = Color.BLUE;    # 2  (namespaced, v1.11.0+)
```
Enum variants are compiled as **global integer variables** (e.g. `Color_RED`).
This is how you represent Rust's C-like enums and named constants. Separators may
be `;` or `,`. `#[non_exhaustive]` has **no equivalent** — Cyrius enums are open
integer sets; just document intended values. Loop-map with `match` or `switch`.

---

## 8. "Enums with data" — tagged unions / sum types (v5.8.21+)

Rust's `enum` with payloads maps to a **heap-allocated tagged union**: a small
`alloc`'d block with the tag at +0 and payloads at +8, +16, …

```
enum Result<T, E> { Ok(v), Err(e) }     # generics accepted but ERASED (i64 only)
enum Option { None(); Some(v); }
enum Tri<T,U,V> { Triple(a,b,c), Pair(x,y), Single(s), Bare }
```

Constructor codegen (auto-generated; **requires `alloc_init()` first**):

```
var ok = Ok(42);        # alloc(16); store64(p,0 /*tag*/); store64(p+8,42); → p
var t  = Triple(1,2,3); # alloc(32); tag@0=0, payload 1/2/3 @ +8/+16/+24
var n  = None();        # alloc(8);  tag@0, NO payload (true nullary)
```
Layout: `alloc(8 + 8*arity)`, tag at +0, `payload[i]` at `+8 + 8*i`. Mixed enums:
**bare names stay integer constants; paren'd names heap-allocate** — keep an enum
paren-consistent for sum types you match on.

Runtime primitives (`lib/tagged.cyr`): `tag(t)` = `load64(t)`,
`payload(t)` = `load64(t+8)`, `is_tag(t, expected)`, `tagged_new(tag, value)`.

**Option** helpers: `is_none`, `is_some`, `unwrap` (aborts on None),
`unwrap_or(opt, fallback)`.

**Result** (`lib/result.cyr`, tag 0 = Ok, 1 = Err): `is_ok`, `is_err_result`,
`result_unwrap` (aborts on Err), `result_unwrap_or`, `err_code_of`, `result_print`.

```
enum Result<T, E> { Ok(v); Err(e); }
fn is_ok(res): i64 { if (load64(res) == Ok) { return 1; } return 0; }
```

**`?` propagation** (v5.8.29+): postfix on a Result-shaped expression — if Err,
returns the Result heap pointer from the enclosing fn; if Ok, unwraps
`load64(p+8)`. Highest precedence. Only valid **inside a fn body**.

```
fn chain(a, b, c) {
    var x = safe_div(a, b)?;    # Err short-circuits
    var y = safe_div(x, c)?;
    return Ok(y);
}
```

**No `try`/`catch`/`throw`/`finally` — ever** (design decision). No unwinding.
`Result` + `?` is the only propagation mechanism; checked-arith overflow
(`+?`) is the only panic-shaped path (`syscall(60,57)`).

---

## 9. Errors

Two idioms coexist; pick per API:

1. **Negative integer error codes** (POSIX/kernel style; the legacy stdlib
   shape). `< 0` means error. `file_open`, raw `syscall`, etc. return `-1` or
   `-errno` on failure. Helper: `fn is_err(ret) { return ret < 0; }`. This is the
   most direct port of Rust error `i32`/enum discriminants when you don't need a
   payload — define an `enum NidhiError { OK=0; BAD_ZONE=1; … }` and return
   negative or enum values.

2. **`Result` tagged union** (§8) — the modern shape, `*_r`-suffixed stdlib fns
   (`file_open_r`, `file_read_r`) return `Result`; pair with per-module typed
   error enums whose variants are module-prefixed (e.g. `IoNotFound`,
   `IoBadFd`). Read the code with `if (load64(res+8) == IoNotFound) { … }`.

For nidhi: mirror `error.rs` `NidhiError` as an `enum NidhiError { … }` of
integer codes, and expose both a legacy `-code` return and (optionally) `Ok/Err`
constructors. There is **no `thiserror`, no `Display`/`Error` trait, no `From`
conversions** — write explicit `nidhi_err_str(code)` mapping fns.

---

## 10. Structs and the `#derive(accessors)` accessors

```
struct Point { x; y; }                 # untyped fields → i64 each
struct Header { magic: i32; ver: i16; flags: i8; _pad: i8; size: i64; }  # widths
struct Rect { tl: Point; br: Point; }  # nested; sizeof sums recursively

var p = Point { 10, 20 };              # positional (declaration order)
var q = Point { x: 10, y: 20 };        # named (any order, ALL fields required)
p.x = 42;                              # field store
var w = r.br.x - r.tl.x;               # chained dot access
```

- Fields are **untyped i64 by default**; per-field width annotations control size
  and offset. **No auto-padding** — order/pad fields yourself for C ABI.
- `sizeof(Point)`, `sizeof(i32)`, etc. are compile-time constants.
- **Field access on a pointer-to-struct auto-derefs** when the local/param is
  annotated `: StructName` (e.g. `fn f(s: Str) { … s.len … }`).
- Struct value **return ABI**: ≤8 B → rax; 9–16 B → rax:rdx pair (auto);
  >16 B → hidden return-pointer. Passing a multi-field struct literal into an
  operator-overload fn passes **addresses**; a single-field struct passes the
  value. Prefer passing `&p` explicitly.

### `#derive(accessors)` — generated getters/setters

```
#derive(accessors)
struct Config { host: Str; port; timeout; }
```
Generates, for each field `f` of struct `Type`:

```
Type_f(p)          # getter → load field f from struct pointer p
Type_set_f(p, v)   # setter → store v into field f of struct pointer p
```
So the above yields `Config_host(p)`, `Config_set_host(p,v)`, `Config_port(p)`,
`Config_set_port(p,v)`, `Config_timeout(p)`, `Config_set_timeout(p,v)`. This is
the idiomatic way to expose the Rust field getters/setters nidhi's `zone.rs` /
`instrument.rs` rely on. **These take a POINTER to the struct.**

### Methods — convention-based, NOT a type system (ADR-004)

`point.scale(2)` desugars to `Point_scale(&point, 2)`. You define:

```
fn Point_scale(self, factor) { … }    # self = pointer to struct, always first arg
point.scale(2);                        # → Point_scale(&point, 2)
```
No overloading (names must be unique per struct), no dynamic dispatch. Port Rust
`impl Point { fn scale(&self, …) }` to a free `fn Point_scale(self, …)`.

### `#derive(Serialize)` — replaces serde

```
#derive(Serialize)
struct Agent { id: i64; name: Str; }
# generates Agent_to_json(&a, sb), Agent_from_json(pairs), Agent_from_json_str(json)
```
Untyped/`iN` fields → JSON numbers; `: Str` fields → quoted strings; nested
`#derive(Serialize)` struct fields → recursive. **Requires** includes:
`syscalls.cyr, string.cyr, str.cyr, alloc.cyr, fmt.cyr` (+ `vec.cyr, hashmap.cyr,
bayan.cyr` for the deserializer). This is the ONLY serde-like facility — there is
no `Serialize`/`Deserialize` trait, no format-generic derive. nidhi's
"every type Serialize + Deserialize" requirement becomes: put `#derive(Serialize)`
on each struct and write roundtrip tests calling `_to_json` / `_from_json_str`.

### `union`

```
union Value { as_int; as_ptr; }   # all fields at offset 0; sizeof = max field
```

---

## 11. Memory model — alloc/free, ownership, sizeof

**No ownership, no borrow checker, no `Drop`, no RAII, no lifetimes.** Memory is
manual. Two allocators:

- **Bump allocator** (`lib/alloc.cyr`, default): `alloc_init()` (call once at
  program start before ANY heap use), `alloc(size)` → pointer (returns 0 on OOM /
  if `alloc_init` was skipped), `alloc_reset()` (frees *everything* at once),
  `alloc_used()`. **There is no per-object `free`** in the bump allocator — you
  reset the whole arena. `arena_new/arena_alloc/arena_reset/arena_free` give
  scoped arenas. An allocator is a struct-of-fnptrs; `alloc_via(a, size)` and
  `default_alloc()` let code be allocator-generic.
- **Freelist allocator** (`lib/freelist.cyr`): O(1) `alloc` + real `free` + reuse,
  when you need individual frees (e.g. a voice pool that recycles).

**Who frees:** whoever owns the arena lifecycle. Idiomatic pattern for nidhi:
`alloc_init()` once; allocate voices/zones/sample buffers from the bump arena or a
freelist pool; `alloc_reset()` at teardown. No destructors run — release OS
resources (fds) explicitly, ideally with `defer`.

Raw memory ops (all take i64 addresses):
```
store8/16/32/64(addr, val)      load8/16/32/64(addr)
var p = &x;   var v = *p;   *p = 99;          # address-of / deref / write
var p: *i64 = &buf; var b = *(p + 1);         # typed ptr: +1 advances 8 bytes
```
`memcpy(dst,src,n)`, `memset(dst,val,n)`, `memeq(a,b,n)`, `memchr` in
`lib/string.cyr`.

`sizeof(Type)` — compile-time byte size; recursive for structs.

**`defer { … }`** (LIFO, runs at fn exit, only if the defer statement was
reached; max 8 per fn) is your RAII replacement for cleanup:
```
fn process(path) {
    var fd = file_open(path, 0, 0);
    defer { file_close(fd); }
    if (err) { return -1; }     # defer runs
    return 0;
}
```

---

## 12. Slices

```
include "lib/slice.cyr"
var s: [u8] = 0;          # bracket form  (or: var t: slice<i64> = 0;)
slice_set(&s, &data, 5);  # ptr@+0, len@+8
var b = s[0];             # bounds-checked, element-width-correct load
var p = s.ptr; var n = s.len;   # fn-local slices only; s.len = 3 truncates view
```
Subscript/dot fire on **fn-local** slices only; top-level slices use helper fns
(`slice_ptr`, `slice_len`, `slice_unchecked_get_W`). A `Str` and a vec header are
byte-compatible with slices.

---

## 13. Functions, multi-return, function pointers, closures

```
fn add(a, b) { return a + b; }       # up to 6 register params, 7+ on stack
fn f(): i64 { return 0; }            # return type annotation optional (all fns
                                     # return an i64; no `void`)
```
- **Forward calls work** — call functions defined later; relaxed ordering
  (functions may appear after statements).
- **Every fn returns an i64.** `return;` (bare) synthesizes `return 0;`. There is
  no `void`; a fn that "returns nothing" returns 0 by convention.
- **Multi-return** (native, v3.7.2): `fn divmod(a,b){ return (a/b, a%b); }` then
  `var q, r = divmod(10,3);` (rax:rdx). Legacy: `ret2(a,b)` + `rethi()`.

**Function pointers:**
```
fn add(a, b) { return a + b; }
fn run() {
    var fp = &add;                    # address of a function
    var r  = callptr(fp, 20, 22);     # 42 — indirect call, any arg count (v6.0.70+)
}
```
`callptr` must be used **inside a function** (needs a frame). Older API:
`fncall0`..`fncall8` from `lib/fnptr.cyr` (`fncall2(&add, 20, 22)`). This is the
basis for vtable dispatch: `callptr(load64(load64(obj) + slot*8), obj, …)`.

**Closures** `|params| body` are anonymous functions whose value is a function
pointer. Lexical capture is **by value** (v6.3.8, heap env object; requires
`alloc_init()`; **not on Windows PE**). Non-capturing closures are bare fn
pointers. Closures must be written **inside a function**. Treat closures as
limited — for nidhi, prefer **named functions + a context struct passed by
pointer** over Rust-style capturing closures.

---

## 14. Program entry, globals, syscalls

- **Entry = top-level statements in source order** (no auto-main). See §1.
- **Globals**: `var g = 42;` at file scope; ~64–1024 globals with initializers
  (cap; use enums for many constants). Enum init runs before global-var init.
- **Syscalls** (Linux x86_64 numbers): `syscall(n, a1, a2, a3, …)`:
  - write: `syscall(1, fd, buf, len)`
  - read:  `syscall(0, fd, buf, len)`
  - exit:  `syscall(60, code)`  (**SYS_EXIT = 60** on Linux; **0** on AGNOS)
  Exit codes truncate to 0–255.

`include "lib/foo.cyr"` is **textual inclusion** (contents replace the line),
relative to cwd. A **missing include for a called fn → UD2 `SIGILL` (exit 132)**
in default mode (the compiler patches the undefined call site to `ud2`).
`cyrius build` auto-prepends resolved deps; `--strict` makes undefined-fn a hard
error.

---

## 15. Attributes and directives

```
#must_use              # warns if the fn's result is dropped at statement level
fn checked(x): i64 { return x * 2; }

#deprecated("use sha256_init")     # warns at every call site (string required)
fn sha1_init() { … }

#derive(accessors)     # generate Type_field / Type_set_field   (§10)
#derive(Serialize)     # generate Type_to_json / _from_json      (§10)

@unsafe { store64(raw_ptr, 0); }   # block marker for ABI-crossing/raw mem ops
```
- There is **no `#[inline]` attribute you write** — `#inline` is not a
  user attribute in the guide; inlining is a compiler decision (small
  straight-line fns become inline candidates automatically, relevant to
  generics). Do not expect a `#[inline]` equivalent; just keep hot fns small.
- **Preprocessor:** `#ifdef/#ifndef/#else/#elif/#endif`; `#ifplat <plat>`
  (`x86_64/aarch64/riscv64/linux/macos/windows/baremetal`) and `#ifplat x86`/
  `aarch64` (→ `CYRIUS_ARCH_*`). `#ref "config.toml"` emits `var key = value;`
  globals at compile time.
- **Mode directives:** `kernel;` (bare-metal ELF), `object;`/`shared;`
  (relocatable `.o`). Not relevant to nidhi as a userland library.

---

## 16. Generics — parsed, mostly erased (do not rely on them like Rust)

```
fn id<T>(x: T): T { return x; }
struct Pair<T> { a: T; b: T; }
```
- A generic fn's base **is** its i64 instantiation (i64-everywhere). Non-i64 type
  args (`add<i32>`, `Box<Point>`) are **monomorphized on demand** and deduped,
  but only for narrow scalars/structs and only for **inline-candidate bodies**
  (≤2 type-bearing params, straight-line — no `if`/`while`/`var`-decl control
  flow). Single type param is the well-tested case; multi-param maps both to the
  first arg.
- **Enum generic params (`<T,E>`) are type-erased** — `Result<T,E>` is just an
  i64-payload tagged union.

**Practical porting rule:** treat Cyrius as if it has **no usable generics** for
anything non-trivial. Monomorphize by hand: write `sample_bank_get`,
`voice_render`, etc. as concrete i64/pointer fns. A Rust `Vec<Zone>` becomes a
`vec` of i64 pointers-to-`Zone`.

---

## 17. Standard library modules nidhi will touch

`include` these (build tool resolves stdlib unprefixed):
```
lib/string.cyr   strlen, streq, memcpy, memset, memchr, strchr, print_num, println, atoi
lib/alloc.cyr    alloc_init, alloc(size), alloc_reset, alloc_used, arena_*, alloc_via
lib/freelist.cyr O(1) alloc + real free + reuse  (voice pools)
lib/str.cyr      Str{data;len}, str_from/new/len/data/eq/cat/sub/print, str_builder_*
lib/vec.cyr      vec_new, vec_push(v,val), vec_pop, vec_get(v,i), vec_set(v,i,x), vec_len, vec_find, vec_remove
lib/hashmap.cyr  map_new (cstr keys), map_new_str (Str keys), map_u64_new (u64 keys)
lib/io.cyr       file_open/read/write/close/read_all + *_r Result variants + IoError enum
lib/fmt.cyr      fmt_int, fmt_hex, fmt_hex0x, fmt_bool, fmt_byte, fmt_int_buf, fmt_float(val,decimals), sprintf
lib/math.cyr     F64_* constants, f64_le/ge/clamp/min/max/lerp, sinh/cosh/tanh/pow, f64_parse
lib/fnptr.cyr    fncall0..fncall8      (or use callptr builtin)
lib/result.cyr   Result<T,E>, Ok/Err, is_ok/is_err_result/result_unwrap/result_unwrap_or/err_code_of
lib/tagged.cyr   Option/Either + tag/payload/is_tag/tagged_new primitives
lib/trait.cyr    manual vtable dispatch (see §18)
lib/simd.cyr     f64v2/f64v4 value + f64v_* flat-array (DSP kernels; PE→ptr forms)
lib/assert.cyr   assert, assert_eq, assert_ne, assert_summary (tests)
lib/bench.cyr    bench_start/end/report, bench_batch_start/stop (sub-µs)
lib/audio.cyr    ALSA PCM via direct ioctls (if nidhi needs live playback out)
```
Test file convention: `.tcyr` in `tests/tcyr/`; a test's exit code must be
`var r = assert_summary(); syscall(60, r);` (assert_summary returns the FAIL
count).

---

## 18. Traits / trait objects — a library pattern, not a language feature

No `trait`, no `dyn`, no `impl Trait`. `lib/trait.cyr` gives **manual vtables**:
a trait object is a 16-byte `{ vtable_ptr; data_ptr; }` fat pointer.

```
fn trait_obj_new(vtable, data): i64 { … }           # {vtable, data}
fn trait_call0(obj, slot): i64 {                    # dispatch fn(data)
    var vt = load64(obj);
    var fp = load64(vt + slot * 8);
    return fncall1(fp, load64(obj + 8));
}
# Build a vtable: var vt = alloc(N*8); store64(vt + i*8, &impl_fn);
```
Port Rust trait objects (`Box<dyn Effect>`, `Box<dyn Filter>`) to: a vtable of
function pointers per concrete type + a `{vtable, data}` fat pointer, dispatched
with `trait_callN` / `callptr`. nidhi's `effect_chain.rs` / naad effect trait
objects become explicit vtables.

---

## 19. Rust features with NO direct Cyrius equivalent — and the workaround

| Rust feature | Cyrius reality | Idiomatic workaround |
|---|---|---|
| Generics `<T>` (real monomorphization) | Parsed, mostly erased; only trivial inline cases monomorphize | Hand-monomorphize: concrete i64/pointer fns per type |
| Traits / `impl Trait` / `dyn Trait` | None in the language | Manual vtables (`lib/trait.cyr`), fat `{vtable,data}` ptr, `callptr` |
| Trait methods / `impl` blocks | Convention-based only | Free fns `Type_method(self, …)`; `p.method()` desugars to that |
| `Option<T>` | Heap tagged union | `enum Option { None(); Some(v); }` + `is_some`/`unwrap_or` (needs alloc) |
| `Result<T,E>` / `?` | Present but E is erased i64 | `Result` + `?`; or negative int codes + `is_err` |
| `derive(Serialize/Deserialize)` (serde) | Only `#derive(Serialize)` → JSON | `#derive(Serialize)` per struct; hand-write other formats |
| Iterators / `.map().filter().collect()` | None | Explicit `for`/`while` loops over `vec`/arrays |
| Closures capturing by ref, `FnMut`/`FnOnce` | Capture is by-value only, no PE, flat | Named fns + a context struct passed by pointer |
| Ownership / borrow / lifetimes / `Drop` | None | Manual alloc; `defer` for cleanup; arena reset |
| `String` vs `&str` | Only cstr + `Str` fat ptr | Use `Str`; no ownership distinction |
| `Vec<T>` of non-i64 | Vec stores i64 slots | Store pointers-to-struct; or packed `iN[]` arrays |
| `enum` with `#[non_exhaustive]` | Open integer set | Plain `enum`; document intended values |
| `panic!`/unwinding/`catch_unwind` | No unwinder | `Result`+`?`; checked-arith `syscall(60,57)`; explicit aborts |
| `From`/`Into`/operator traits | None (no overload resolution across types) | Explicit conversion fns (`f64_from`, `str_from`, `nidhi_err_str`) |
| `#[inline]` you control | Not a user attribute | Keep hot fns small (auto inline-candidate); no annotation |
| `const fn` / `const` | No `const` | `var` globals, enum constants, `sizeof()`, precomputed hex |
| f32 arithmetic | Convert-only | Widen `f32_to`→f64, compute, `f32_from`→f32 |
| Negative literals `-5` | Not lexable | `(0 - 5)` |
| `++`/`--`, `for(;;)` | Absent | `i += 1`; `while (1 == 1)` |

---

## 20. Porting checklist for nidhi's modules (quick map)

- `error.rs` → `enum NidhiError { OK=0; … }` + `nidhi_err_str(code)`; return
  negative codes and/or `Result` (Ok/Err).
- `sample.rs` / `capture.rs` → sample data as `alloc`'d `i32[]`/`i16[]` buffers
  (PCM) wrapped in a `struct Sample { data; len; rate; … }`; DSP in f64 via
  `f64_from`/`f64_to`. `SampleBank` → `vec` of `Sample*`.
- `zone.rs` / `instrument.rs` → structs with `#derive(accessors)` for
  field access; round-robin index is a plain i64 field.
- `engine.rs` voice management → a freelist pool of `Voice*`; render loop is a
  `while`/`for` over active voices, no iterators.
- `loop_mode.rs` → `enum LoopMode { OneShot=0; Forward=1; PingPong=2; Reverse=3;
    LoopSustain=4; }` (plain int enum) + `switch`/`match` in the render fn.
- `envelope.rs` / LFO → f64 bit-pattern math (`F64_*` constants, `f64_add/mul`).
- `effect_chain.rs` → manual vtable per effect (`lib/trait.cyr`).
- serde roundtrip tests → `#derive(Serialize)` + `.tcyr` tests asserting
  `_from_json_str(_to_json(x)) == x`.
- Every "Serialize + Deserialize" invariant → `#derive(Serialize)` on the struct.
- `#[must_use]` on accessors → `#must_use` fn attribute (statement-drop warning
  only).

**Non-negotiable runtime rules:** call `alloc_init()` once before any `alloc`,
`Ok(...)`, `Some(...)`, closure capture, vec, or Str; every `.tcyr` test exits
via `syscall(60, assert_summary())`; missing includes = SIGILL 132; use
`var a: i64[N]` (never bare `var a[N]`) for slot arrays.
