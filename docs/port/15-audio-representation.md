# 15 — Canonical Audio Representation for the Cyrius Port

Read-only reconnaissance. This brief fixes the canonical way audio data is
represented in the already-ported Cyrius audio ecosystem (**naad** synthesis,
**shravan** codecs), plus the DSP idioms from **vidya**, so that nidhi's
`Sample` / `SampleBank` / voice buffers match what its consumers expect.

Sources studied (absolute paths):

- `/home/macro/Repos/naad/src/voice.cyr`, `error.cyr`, `dsp_util.cyr`,
  `filter.cyr`, `envelope.cyr`, `panning.cyr`, `granular.cyr`, `wavetable.cyr`,
  `main.cyr`
- `/home/macro/Repos/shravan/src/resample.cyr`, `shravan.cyr`, `flac.cyr`,
  `serde.cyr`
- `/home/macro/Repos/vidya/content/audio_dsp/cyrius.cyr` + `concept.toml`
- `/home/macro/Repos/vidya/content/audio_synthesis/cyrius.cyr`
- `/home/macro/Repos/vidya/content/fixed_point_arithmetic/concept.toml`
- `/home/macro/Repos/vidya/content/performance/cyrius.cyr`
- Rust being ported: `/home/macro/Repos/nidhi/src/sample.rs`

---

## 0. TL;DR — the decisions

| Question | Decision |
|---|---|
| Sample value type | **f64 bit-patterns** (NOT f32, NOT Q15) — matches naad + shravan |
| Buffer container | stdlib **`vec`** of f64 slots (one f64 per slot), created with `vec_new`/`vec_push`, read with `vec_get`, written with `vec_set`, sized with `vec_len` |
| Stereo / multichannel | **interleaved** in a single `vec` (L,R,L,R…); channel count stored as a separate i64 struct field |
| `Sample` | heap `#derive(accessors)` struct holding the `vec` handle + i64 metadata |
| `SampleBank` | struct wrapping a `vec` of `Sample` pointers; `SampleId` is a plain i64 index |
| Per-voice output | for the hot path prefer **raw f64 samples returned by value** (no per-sample alloc); accumulate into a caller-supplied `vec<f64>` buffer, exactly like `filter_biquad_process_buffer` / `granular_fill_buffer` |
| Interpolation | linear = manual `s0*(1-frac)+s1*frac`; cubic = call naad's `hermite_interpolate(y0,y1,y2,y3,t)` from `dsp_util.cyr` |
| Ownership | bump allocator, **no free**. Allocate large sample buffers once at load; never alloc inside a per-sample loop |

Vidya's Q15 fixed-point corpus (`ONE=32768`, `q_mul`) is **didactic only** — the
production libraries naad/shravan are f64 throughout. Follow naad/shravan.

---

## 1. The everything-is-i64 float model

Cyrius has no float type. An f64 is a 64-bit slot holding the **IEEE-754 bit
pattern**; arithmetic goes through builtins. From naad/shravan the vocabulary is:

- Construct from an integer: `f64_from(440)` → the f64 for 440.0.
  Truncate back to integer: `f64_to(x)` (truncates toward zero, like Rust `as`).
- Arithmetic: `f64_add f64_sub f64_mul f64_div f64_neg f64_abs`.
- Compare (return **1/0**, not bool): `f64_lt f64_le f64_gt f64_ge f64_eq`.
- Math: `f64_sin f64_cos f64_tan f64_sqrt f64_pow f64_ln f64_floor f64_ceil
  f64_clamp f64_min f64_tanh f64_atan`.
- Named constants (from the math/ganita stdlib used by naad):
  `F64_ZERO F64_ONE F64_TWO F64_HALF F64_PI F64_PI_2 F64_TAU F64_LN10`.
- Raw hex literals are used when a constant isn't in the stdlib, e.g. naad's
  `var DSP_C2_5 = 0x4004000000000000;   # 2.5`. Only reach for these for odd
  coefficients; prefer `f64_from` + arithmetic otherwise.

Two independent number systems live in i64 slots and must never be mixed
blindly: **f64 bit-patterns** (sample values, gains, positions) vs **plain
integers** (indices, counts, sample rate in Hz, channel counts, MIDI notes,
error codes). Sample *rate* is stored as a **plain integer** (Hz), matching
shravan's `fmtinfo_sample_rate` (`load64(fi+8)`, a raw i64) and
`resample(...source_rate, target_rate...)` taking integer Hz. Convert to f64
only where you do float math on it: `f64_from(sample_rate)`.

### Why f64, not f32, and not Q15

- naad's port note is explicit (`error.cyr`): *"naad's Rust source was f32; the
  Cyrius port is f64 throughout (hisab's HVec3/HComplex are f64-only)."* The
  whole ecosystem promoted f32→f64. nidhi's Rust `Vec<f32>` sample data
  therefore becomes **f64** slots, not f32.
- shravan decodes every codec to **interleaved f64** normalized to [-1, 1]
  (see §3). If nidhi stored f32 or Q15 it would have to convert on every load.
- There is no packed-f32 storage in the ecosystem; one f64 per `vec` slot is the
  universal layout. Do not try to pack two f32 per i64.
- vidya's Q15 (`audio_dsp`, `audio_synthesis`) is a portability teaching device
  ("bit-exact across languages"). The real libraries chose f64 for dynamic range
  and to match hisab. **Use f64.**

---

## 2. The canonical buffer: a `vec` of f64

Every audio buffer in naad and shravan is a stdlib `vec` where **each slot holds
one f64 sample bit-pattern**. This is stated verbatim in
`shravan/src/resample.cyr`: *"All samples stored as f64 bit patterns in vecs."*

Stdlib `vec` API used everywhere (untyped — slots are raw i64 holding f64
patterns):

```
var buf = vec_new();          # empty growable buffer
vec_push(buf, f64_from(0));   # append a sample
var n   = vec_len(buf);       # length (integer count of samples)
var s   = vec_get(buf, i);    # read slot i  (an f64 bit-pattern)
vec_set(buf, i, f64_mul(s, g));  # write slot i
```

Per-sample DSP loops are written as plain `while` loops over `vec_get`/`vec_set`.
Canonical examples:

- Gain/normalize (`naad/dsp_util.cyr` `normalize`): find peak, then
  `vec_set(buffer, i, f64_mul(vec_get(buffer, i), inv))`.
- In-place filter over a buffer (`naad/filter.cyr`
  `filter_biquad_process_buffer`):

```
#inline
fn filter_biquad_process_buffer(self, buffer) {
    var n = vec_len(buffer);
    var i = 0;
    while (i < n) {
        vec_set(buffer, i, filter_biquad_process_sample(self, vec_get(buffer, i)));
        i = i + 1;
    }
    return 0;
}
```

### Raw-pointer buffers (the codec/DSP-internal alternative)

Inside shravan codecs, tight fixed-size scratch buffers are raw `alloc`'d i64
arrays addressed with `store64`/`load64` at byte offsets (`store64(buf + i*8,
val)`), e.g. `flac_decode_lpc`. vidya's `audio_dsp/cyrius.cyr` FIR history and
`sample_buf` do the same (`load64(buffer + i*8)`). **Use raw `alloc` +
`store64`/`load64` only for fixed-size internal scratch** (filter history rings,
FFT scratch). For anything that is loaded, stored on a struct, grown, or handed
across an API boundary, **use `vec`** — it is the ecosystem's public currency and
what shravan's `decode_result` hands you.

### Interleaving convention (stereo / multichannel)

Multichannel audio is **interleaved in one flat `vec`**: frame f, channel c lives
at slot `f * channels + c`. This is shravan's universal convention:

- `shravan/src/shravan.cyr` `interleave(channels, ch_count)` pushes
  `vec_get(ch, fr)` for each channel per frame → single interleaved `vec`.
- `deinterleave(samples, ch_count)`: `frames = total / ch_count`, reads
  `vec_get(samples, fr * ch_count + c)`.
- `resample(samples_vec, channels, ...)` treats its input as *"interleaved f64
  samples"* and computes `frames = total / channels`.

This exactly matches nidhi's Rust: `Sample::from_stereo` sets
`frames = data.len() / 2` with interleaved data, and `read_stereo_frame` indexes
`data[i*ch]`, `data[i*ch+1]`. So `total_slots = frames * channels`.

---

## 3. What shravan hands nidhi (the load path)

When nidhi loads a WAV/FLAC/etc via shravan, it gets a `decode_result` — a
16-byte struct holding `{fmtinfo_ptr, samples_vec}`:

```
fn decode_result_new(fi, samples) { var dr = alloc(16); store64(dr, fi); store64(dr+8, samples); return dr; }
fn decode_result_info(dr)    { return load64(dr); }       # -> fmtinfo pointer
fn decode_result_samples(dr) { return load64(dr + 8); }   # -> vec<f64>, interleaved, [-1,1]
```

`fmtinfo` (48-byte struct, `shravan.cyr` ~L148) carries the metadata nidhi needs:

```
fn fmtinfo_sample_rate(fi)  { return load64(fi + 8);  }   # integer Hz
fn fmtinfo_channels(fi)     { return load64(fi + 16); }   # integer
fn fmtinfo_bit_depth(fi)    { return load64(fi + 24); }   # integer
fn fmtinfo_total_samples(fi){ return load64(fi + 40); }   # integer frame count
```

Codecs normalize integer PCM to f64 in [-1,1] by dividing by
`2^(bit_depth-1)` (`shravan.cyr` ALAC path):
`vec_push(output, f64_div(f64_from(raw), scale))` where
`scale = f64_pow(F64_TWO, f64_from(bit_depth - 1))`.

**Implication for nidhi:** `Sample` construction from a load is essentially free —
take `decode_result_samples(dr)` directly as the `Sample`'s `data` vec, and
`fmtinfo_sample_rate` / `fmtinfo_channels` as the integer metadata. No f32↔f64,
no re-normalization. To change playback rate, hand the vec to
shravan `resample(samples, channels, src_rate, tgt_rate, RESAMPLE_GOOD)`.

---

## 4. Recommended nidhi structs

### 4a. `Sample` (ports `nidhi/src/sample.rs` `Sample`)

Cyrius structs are heap-allocated via `alloc(sizeof(T))` with **untyped fields**;
`#derive(accessors)` generates `Sample_field(p)` / `Sample_set_field(p, v)`.
This is exactly naad's `struct Voice { active; note; ... }` and
`struct Wavetable { samples; }` pattern. Field types (all stored in i64 slots):

```
# data        : vec<f64>, interleaved samples (f64 bit-patterns)
# channels    : integer   (1 = mono, 2 = stereo)
# sample_rate : integer   Hz
# frames      : integer   samples-per-channel  (== vec_len(data) / channels)
# name        : Str pointer (empty string if unnamed)
# slices      : vec<integer>  REX-style slice points (frame indices)
#derive(accessors)
struct Sample { data; channels; sample_rate; frames; name; slices; }
```

Constructors mirror the Rust (`from_mono` / `from_stereo`). `frames` is derived,
same as Rust:

```
#must_use
fn sample_from_mono(data, sample_rate) {
    var s = alloc(sizeof(Sample));
    Sample_set_data(s, data);
    Sample_set_channels(s, 1);
    Sample_set_sample_rate(s, sample_rate);   # integer Hz
    Sample_set_frames(s, vec_len(data));
    Sample_set_name(s, "");
    Sample_set_slices(s, vec_new());
    return s;
}

#must_use
fn sample_from_stereo(data, sample_rate) {
    var s = alloc(sizeof(Sample));
    Sample_set_data(s, data);
    Sample_set_channels(s, 2);
    Sample_set_sample_rate(s, sample_rate);
    Sample_set_frames(s, vec_len(data) / 2);   # interleaved: 2 slots per frame
    Sample_set_name(s, "");
    Sample_set_slices(s, vec_new());
    return s;
}
```

Note on `#[non_exhaustive]` enums and serde: nidhi's Rust derives
`Serialize/Deserialize` on `Sample`/`SampleBank`. **Cyrius has no serde/generics**
(naad drops all serde derives; shravan hand-writes a `serde.cyr` when a format
genuinely needs on-disk bytes). Drop the derives; if wire serialization is
actually required, follow shravan's `serde.cyr` approach of an explicit
byte-emitter, not a derive. (`shravan/serde.cyr` is the one place using *typed*
struct fields like `title: Str;` — that's the exception, driven by its
serializer; naad and the DSP hot paths use untyped fields. For nidhi, use untyped
fields like naad.)

### 4b. `SampleBank` (ports `SampleBank`, keyed by `SampleId`)

Rust `SampleId(u32)` → a **plain integer index**. Rust `Vec<Sample>` → a `vec`
of `Sample` pointers. This is exactly naad's `VoiceManager { voices; ... }`
holding a `vec` of `Voice` pointers.

```
#derive(accessors)
struct SampleBank { samples; }   # samples : vec of Sample pointers

#must_use
fn sample_bank_new() {
    var b = alloc(sizeof(SampleBank));
    SampleBank_set_samples(b, vec_new());
    return b;
}

# add -> returns the new SampleId (the index), matching Rust add().
fn sample_bank_add(b, sample) {
    var samples = SampleBank_samples(b);
    var id = vec_len(samples);         # SampleId = current length
    vec_push(samples, sample);
    return id;
}

# get -> Sample pointer, or 0 (null) sentinel when out of range.
# Mirror naad's voice_manager_voice_mut null-return idiom (Rust Option -> 0).
#inline
fn sample_bank_get(b, id) {
    var samples = SampleBank_samples(b);
    if (id < 0) { return 0; }
    if (id >= vec_len(samples)) { return 0; }
    return vec_get(samples, id);
}
```

`Option<&Sample>` → return the pointer, or **0 as the null/None sentinel**
(naad's established convention: `voice_manager_voice_mut` returns `0` for
out-of-bounds; `Option<usize>` uses `-1` = `VOICE_NONE`). For a pointer-returning
lookup, 0 is the natural None; for an index-returning function use `-1`.

### 4c. Per-voice render / output buffer

Rust `read_stereo_interpolated` returns a `(f32, f32)` tuple. Cyrius has no
tuples; naad's fix (see `panning.cyr`) is a tiny 2-field struct:

```
#derive(accessors)
struct StereoFrame { left; right; }   # both are f64
```

**But do not alloc one per sample in a render loop.** naad's `panning.cyr` is
explicit about this: `panning_pan_mono` allocates the result `PanPair` but
computes gains into *locals* to avoid an intermediate alloc, with the comment
*"this is a per-sample path and Cyrius has no free."* And `filter.cyr` provides
`_filter_svf_compute_lowpass` as a value-returning variant precisely *"so the
bump allocator does not grow per sample."*

Recommended per-voice output strategy (matches `granular_fill_buffer` and
`filter_biquad_process_buffer`):

- Allocate the voice's output `vec<f64>` (or L and R vecs, or one interleaved
  vec) **once** when the voice starts.
- The inner render loop **accumulates into that pre-sized buffer** via
  `vec_set`, or returns bare f64 sample values and lets the mixer sum them —
  never allocating inside the loop.
- For a stereo per-sample read, prefer two return values via out-pointers
  (`store64(out_l, l); store64(out_r, r)`) or write directly into interleaved
  output slots, rather than allocating a `StereoFrame` each sample.

---

## 5. The hot per-sample render loop (idiom + `#inline`)

Attribute conventions from naad (apply to nidhi identically):

- `#inline` on hot per-sample functions (`filter_biquad_process_sample`,
  `envelope_adsr_next_value`, `panning_pan_mono`, `hermite_interpolate`, `lerp`).
- `#must_use` on pure/accessor functions.
- `#derive(accessors)` on every public struct.

Skeleton for a voice render pass — read interpolated from the `Sample`, apply
envelope and gain, write to the output buffer. Everything f64, no alloc inside:

```
#inline
fn nidhi_voice_render(voice, sample, out_buf) {
    var data     = Sample_data(sample);
    var channels = Sample_channels(sample);
    var frames   = Sample_frames(sample);
    var n        = vec_len(out_buf);
    var pos      = NidhiVoice_position(voice);     # f64 fractional frame index
    var rate     = NidhiVoice_playback_rate(voice); # f64 frames advanced per output sample
    var env      = NidhiVoice_env(voice);           # Adsr pointer (naad envelope.cyr)

    var i = 0;
    while (i < n) {
        # cubic-hermite read at the fractional position (see §6)
        var s = nidhi_sample_read_cubic(sample, pos);
        var e = envelope_adsr_next_value(env);      # naad ADSR, f64 in [0,1]
        var g = NidhiVoice_gain(voice);             # f64
        var mixed = f64_add(vec_get(out_buf, i), f64_mul(f64_mul(s, e), g));
        vec_set(out_buf, i, mixed);
        pos = f64_add(pos, rate);                   # advance read head (pitch)
        i = i + 1;
    }
    NidhiVoice_set_position(voice, pos);
    return 0;
}
```

Key idioms this mirrors:
- Accumulate-into-caller-buffer (`granular_fill_buffer`, `..process_buffer`).
- State written back to the struct *after* the loop, not every field every
  sample where avoidable.
- Comparisons return 1/0; there is no bool. `if (f64_lt(a, b) == 1) { ... }`.
- `elif` exists (used heavily in `envelope.cyr`); `else` exists.

### Denormal flushing

naad flushes denormals in every feedback path (`flush_denormal` in `error.cyr`,
called by both filter cores). If nidhi feeds sample output through naad filters
this is handled inside naad; if nidhi has its own decaying feedback/gain smoothing
state, wrap it in `flush_denormal(x)` the same way — subnormals cause 10–100×
slowdowns.

---

## 6. Interpolation for pitch shifting

nidhi's Rust `read_cubic` / `read_stereo_interpolated` use cubic Hermite
(Catmull-Rom). **naad already provides the identical formula** — reuse it, don't
re-derive:

`naad/src/dsp_util.cyr`:

```
#inline
#must_use
fn lerp(a, b, t) {                       # linear
    return f64_add(a, f64_mul(f64_sub(b, a), t));
}

#inline
#must_use
fn hermite_interpolate(y0, y1, y2, y3, t) {   # cubic Hermite / Catmull-Rom
    var c0 = y1;
    var c1 = f64_mul(F64_HALF, f64_sub(y2, y0));
    var c2 = f64_sub(f64_add(f64_sub(y0, f64_mul(DSP_C2_5, y1)), f64_mul(F64_TWO, y2)), f64_mul(F64_HALF, y3));
    var c3 = f64_add(f64_mul(F64_HALF, f64_sub(y3, y0)), f64_mul(DSP_C1_5, f64_sub(y1, y2)));
    var r = f64_add(f64_mul(c3, t), c2);
    r = f64_add(f64_mul(r, t), c1);
    r = f64_add(f64_mul(r, t), c0);
    return r;
}
```

This is **coefficient-identical** to nidhi's `Sample::cubic_hermite`
(`a=-0.5y0+1.5y1-1.5y2+0.5y3`, `b=y0-2.5y1+2y2-0.5y3`, `c=-0.5y0+0.5y2`, `d=y1`,
Horner-evaluated) — same Catmull-Rom, same Horner form. Drop nidhi's SSE variant;
there is no packed-f32 SIMD in the Cyrius ecosystem. Compute L and R with two
scalar `hermite_interpolate` calls.

### The read idiom (fractional position → 4 taps → hermite)

The canonical fractional-read shape is naad `granular.cyr` (rendering with
wraparound) and `wavetable_read_interpolated` (linear, with wrap). For nidhi's
**one-shot / clamped** reads (Rust `read_mono_frame` returns 0 out of bounds,
does not wrap), replicate the Rust clamp semantics rather than wrap:

```
# read one mono value at integer frame idx; 0.0 out of bounds; stereo -> (L+R)*0.5
#inline
#must_use
fn nidhi_read_mono_frame(sample, idx) {           # idx is a plain integer
    var frames = Sample_frames(sample);
    if (idx < 0)       { return F64_ZERO; }
    if (idx >= frames) { return F64_ZERO; }
    var data = Sample_data(sample);
    var ch   = Sample_channels(sample);
    if (ch == 1) { return vec_get(data, idx); }
    var l = vec_get(data, idx * ch);
    var r = vec_get(data, idx * ch + 1);
    return f64_mul(f64_add(l, r), F64_HALF);
}

#inline
#must_use
fn nidhi_sample_read_cubic(sample, position) {    # position is an f64
    if (Sample_frames(sample) == 0) { return F64_ZERO; }
    var floor_pos = f64_floor(position);
    var idx  = f64_to(floor_pos);                 # integer frame
    var frac = f64_sub(position, floor_pos);       # f64 in [0,1)
    var y0 = nidhi_read_mono_frame(sample, idx - 1);
    var y1 = nidhi_read_mono_frame(sample, idx);
    var y2 = nidhi_read_mono_frame(sample, idx + 1);
    var y3 = nidhi_read_mono_frame(sample, idx + 2);
    return hermite_interpolate(y0, y1, y2, y3, frac);
}
```

Watch-outs (from vidya `fixed_point_arithmetic` gotchas — they apply to the
*integer* index math even in the f64 world):

- `f64_to` **truncates toward zero**, so for a negative `position` use `f64_floor`
  *before* `f64_to` (as above) to get true floor, not truncation. Rust uses
  `position.floor() as isize` for this exact reason.
- Accumulate the read head `pos` in **full f64** (`pos = f64_add(pos, rate)`);
  never round-trip through integers per sample or you lose sub-sample precision
  over the length of a note (vidya: "Accumulating small values loses precision").
- Playback rate for pitch: `rate = f64_div(f64_from(sample_sr), f64_from(engine_sr))`
  times the pitch ratio; when resampling to a fixed engine rate up front, prefer
  shravan `resample` (windowed-sinc, higher quality) over per-sample cubic.

---

## 7. Memory ownership for large sample buffers

- **Bump allocator, no free.** `alloc(n)` returns `n` bytes; there is no
  `free`/`dealloc` (vidya `performance/cyrius.cyr`: heap is "bump alloc";
  `alloc_reset` wholesale-resets in tests only). Every naad hot path is written
  to *avoid* per-sample allocation for this reason.
- **Allocate sample data once, at load.** A `Sample`'s `data` vec is created when
  the file is decoded (ideally taken straight from `decode_result_samples`) and
  lives for the program's life. Do not copy it per voice — voices hold a *pointer*
  to the shared `Sample` and their own small f64 read-head state, exactly like
  naad's `Grain` holds `source_position` into a shared `GranularEngine.source`
  vec. Rust's `Clone` on `Sample` should become a pointer share, not a deep copy,
  unless a genuine independent copy is needed.
- **`vec` growth reallocates**; sizing a `vec` up front (push zeros, as
  `resample_mono` does: pre-fill `out` with `F64_ZERO`) avoids repeated growth in
  the render path. For fixed-size per-voice output, pre-size once.
- `sizeof(T)` gives a struct's byte size for `alloc(sizeof(Sample))`. Raw scratch
  arrays are `alloc(count * 8)` (8 bytes per i64/f64 slot), addressed
  `load64(p + i*8)` / `store64(p + i*8, v)`.

---

## 8. Direct Rust → Cyrius mapping table for `sample.rs`

| Rust (`sample.rs`) | Cyrius |
|---|---|
| `Vec<f32> data` | `vec` of f64 slots (interleaved) |
| `f32` sample value | f64 bit-pattern |
| `u32 channels / sample_rate`, `usize frames` | plain integers (i64 slots) |
| `String name` | `Str` pointer (or `""`) |
| `Vec<usize> slices` | `vec` of integers |
| `SampleId(u32)` | plain integer index |
| `Option<&Sample>` (get) | pointer, or `0` = None |
| `(f32, f32)` stereo | `StereoFrame {left; right;}` struct, or two out-pointers on the hot path |
| `Sample::cubic_hermite` | naad `hermite_interpolate` (identical coeffs) — reuse |
| `read_mono_frame` clamp-to-0 | `nidhi_read_mono_frame` (bounds → `F64_ZERO`) |
| `#[inline]` / `#[must_use]` | `#inline` / `#must_use` |
| `#[derive(Serialize, Deserialize)]` | **dropped** (no serde); hand-write per shravan `serde.cyr` only if on-disk bytes are truly needed |
| SSE `cubic_hermite_stereo_sse` | **dropped**; two scalar hermite calls |
| `position.floor() as isize` | `f64_to(f64_floor(position))` (floor BEFORE truncate) |
