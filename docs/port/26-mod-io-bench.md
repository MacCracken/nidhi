# Port Spec 26 — `io` module + benchmark/fuzz harnesses (Rust → Cyrius)

Read-only recon for the Cyrius port of **nidhi**. Covers:
`src/io.rs` (WAV load/stream via shravan), `benches/benchmarks.rs` (parity
benchmarks), `fuzz/fuzz_targets/` (fuzz harnesses), `Makefile`, and
`docs/development/roadmap.md`.

**Huge advantage:** shravan is *already ported to Cyrius* at
`/home/macro/Repos/shravan` (`.cyr` sources + `cyrius.cyml`/`cyrius.lock`).
Every shravan function nidhi's `io.rs` calls already exists in Cyrius. This
brief maps the Rust calls → the concrete Cyrius functions and struct layouts to
call, so you do not re-derive them.

---

## 0. Cyrius idioms crib (from shravan `.cyr`)

- Everything is `i64`. Heap structs are `alloc(bytes)` + `store64(ptr+off, v)` /
  `load64(ptr+off)`. Floats are f64 bit-patterns: `f64_from(int)`, `f64_add`,
  `f64_mul`, `f64_div`, `f64_sin`, etc. **f32 does not exist as a value type** —
  32-bit float bytes are decoded/encoded via helpers (`f32_to_f64_vec`,
  `f64_to_f32_bits`).
- Errors are **negative integers**. shravan uses `fn err(code){ return 0-code; }`
  and `is_err(r){ return r<0; }`; `err_code(r) = 0-r`. Codes (`enum ShravanErr`):
  `ERR_NONE=0, ERR_UNSUPPORTED_FMT=1, ERR_INVALID_HEADER=2, ERR_DECODE=3,
  ERR_ENCODE=4, ERR_END_OF_STREAM=5, ERR_INVALID_RATE=6, ERR_INVALID_CHANNELS=7`.
- Structs may be declared `struct Name { field; field: Str; }` (untyped fields,
  optional `: Str` hint) OR built ad-hoc as raw `alloc`+offset blocks. shravan's
  codec structs use the raw offset style (documented with a byte-layout comment);
  mirror that for nidhi's `Sample`, `StreamingWavReader`, etc. `#derive(accessors)`
  generates `Type_field`/`Type_set_field`; where shravan hand-writes accessors
  (`fmtinfo_channels(fi)`) you may instead use the derive — pick one convention
  and keep it.
- `vec` API (`lib/vec.cyr`): `vec_new()`, `vec_push(v,val)`, `vec_get(v,idx)`,
  `vec_set(v,idx,val)`, `vec_len(v)`. Vecs hold i64 slots; a "vec of f32" is a
  vec of **f64 bit-patterns** in shravan.
- No serde/generics/trait-objects. No `Result<T,E>` enum — the "packed result"
  convention is: `>=0` = success (often a pointer or count), `<0` = error code.

---

## 1. `src/io.rs` — public surface (behind `io` feature ⇒ implies `std`)

Rust file: `/home/macro/Repos/nidhi/src/io.rs`. Error type is
`crate::error::NidhiError`; **all failures here become
`NidhiError::ImportError(String)`** (variant at `src/error.rs:22`;
`Result<T> = core::result::Result<T, NidhiError>` at `src/error.rs:45`). In
Cyrius, `ImportError` collapses to a negative code — reserve one nidhi error
constant, e.g. `NIDHI_ERR_IMPORT`, and return `0 - NIDHI_ERR_IMPORT` on any I/O
or decode failure (the human-readable `format!(...)` message text has no Cyrius
equivalent and is dropped).

### 1.1 Public functions / types

| Rust item | Signature | Purpose |
|---|---|---|
| `io::load_wav<P: AsRef<Path>>(path) -> Result<Sample>` | reads file, decodes, names sample from file stem | load whole WAV from disk |
| `io::load_wav_from_memory(data: &[u8]) -> Result<Sample>` | decode in-memory bytes | load WAV from a byte slice |
| `struct StreamingWavReader` | see fields below | chunked streaming of large WAVs |
| `StreamingWavReader::open<P>(path) -> Result<Self>` | reads file, decodes header for metadata | open for streaming |
| `.sample_rate() -> u32` / `.channels() -> u32` / `.total_frames() -> usize` / `.frames_read() -> usize` | `#[must_use]` accessors | metadata |
| `.read_chunk(&mut self, chunk_frames: usize) -> Result<Vec<f32>>` | feed 4096-byte file slices → stream events → interleaved f32 | pull next chunk |
| `.read_all(mut self) -> Result<Sample>` | reads `total_frames` then builds Sample | consume whole reader |

`StreamingWavReader` fields (Rust) → Cyrius heap struct layout (suggest 80 bytes,
i64 slots):

```
+0  decoder        (shravan WAV stream handle — decode_reader(chunk_frames) / wav_stream_new)
+8  info           (FormatInfo ptr, or 0 = None)
+16 pending_samples(vec ptr; f64 bit-patterns of interleaved samples not yet drained)
+24 channels       (i64)
+32 sample_rate    (i64)
+40 total_frames   (i64)
+48 frames_read    (i64)
+56 finished       (i64 bool: 0/1)
+64 file_data      (ptr to full file byte buffer)
+72 file_len       (i64; Rust used a Vec — carry length separately)
    (file_offset)  (add +80 slot; Rust field `file_offset: usize`)
```

### 1.2 Sample constructors this module depends on

From `src/sample.rs` (their own port spec covers these; signatures here for the
boundary):

- `Sample::from_mono(data: Vec<f32>, sample_rate: u32) -> Sample`  (`sample.rs:32`)
- `Sample::from_stereo(data: Vec<f32>, sample_rate: u32) -> Sample`  (`sample.rs:45`)
  — `data` is **interleaved L,R,L,R…**; `frames = data.len()/2`.
- `Sample::with_name(self, name: impl Into<String>) -> Sample`  (`sample.rs:58`)
- accessors: `.channels()->u32` (`:141`), `.sample_rate()->u32` (`:148`),
  `.frames()->usize` (`:155`).

In Cyrius the `Vec<f32>` argument is a **vec of f64 bit-patterns** (shravan
already returns samples that way), so the f32↔f64 conversion the Rust code does
implicitly at the `shravan::wav::decode` boundary is a **no-op** on the Cyrius
side — pass shravan's samples vec straight into `sample_from_mono/stereo`.

### 1.3 shravan calls — Rust → Cyrius mapping

Cyrius sources: `/home/macro/Repos/shravan/src/shravan.cyr` and `src/stream.cyr`.

| Rust call (io.rs) | Cyrius function | Returns |
|---|---|---|
| `shravan::wav::decode(&data) -> (FormatInfo, Vec<f32>)` | `wav_decode(data, len)` (`shravan.cyr:597`) | `decode_result` ptr (`<0`=err). `decode_result_info(dr)`→FormatInfo ptr, `decode_result_samples(dr)`→samples vec |
| `shravan::wav::encode(samples, rate, ch, PcmFormat::F32)` *(tests only)* | `wav_encode(samples, sample_rate, channels, pcm_fmt, out)` (`shravan.cyr:723`) — writes into caller `out` buffer, returns bytes written (`<0`=err). `pcm_fmt` = `PCM_F32` for F32 | i64 byte count |
| `info.channels` (`u16`) | `fmtinfo_channels(fi)` (`shravan.cyr:170`) | i64 |
| `info.sample_rate` (`u32`) | `fmtinfo_sample_rate(fi)` (`shravan.cyr:169`) | i64 |
| `info.total_samples` (`usize`) → `total_frames` | `fmtinfo_total_samples(fi)` (`shravan.cyr:173`) | i64. **NOTE:** shravan's field is already **frames**, not sample count — Rust names it `total_samples` but assigns it to `total_frames` (io.rs:95). Do not re-divide by channels. |
| `shravan::stream::WavStreamDecoder::new()` | `wav_stream_new(chunk_frames)` (`stream.cyr:50`) — or the format-detecting `decode_reader(chunk_frames)` (`shravan.cyr:1752`) | stream handle ptr |
| `decoder.feed(chunk) -> Vec<StreamEvent>` | `wav_stream_feed(dec, data, len)` (`stream.cyr:119`) / `decode_reader_feed(dr,data,len)` (`shravan.cyr:1767`) | vec of event ptrs |
| `decoder.flush() -> Vec<StreamEvent>` | `wav_stream_flush(dec)` (`stream.cyr:127`) / `decode_reader_flush(dr)` (`shravan.cyr:1806`) | vec of event ptrs |
| `std::fs::read(path)` | `decode_file`-style read (`shravan.cyr:1712`): `file_open(path,0,0)`, loop `file_read`, `file_close`. Returns buffer+len | ptr / err |

**FormatInfo** (shravan `fmtinfo`, 48-byte struct, `shravan.cyr:148-173`):
```
+0  format(AudioFormat enum)  +8  sample_rate  +16 channels  +24 bit_depth
+32 duration_secs (f64 bits)  +40 total_samples (== frames)
```

**decode_result** (16 bytes, `shravan.cyr:585`):
`+0 fmtinfo ptr, +8 samples vec ptr`. `wav_decode` returns `<0` on failure.

**StreamEvent** (shravan stream, 16 bytes, `stream.cyr:32-40`):
```
+0 type   (0=HEADER →data is FormatInfo ptr; 1=SAMPLES →data is samples vec;
           2=END →data is 0)
+8 data_ptr
```
`stream_evt_type(evt)` / `stream_evt_data(evt)` are the accessors. Rust's
`match event { Header(info)|Samples(s)|End|_ }` becomes an if-ladder on
`stream_evt_type`. Rust's `WavStreamDecoder` (Rust side) also exposes a variant
the code ignores (`_ => {}`); Cyrius only has the three above, so no default arm
needed.

### 1.4 Behaviour to reproduce exactly

- `load_wav`: read file → `wav_decode` → if `channels==1` build mono else stereo
  → set name = **file stem** (basename without extension; empty string if none).
  Cyrius has no `Path::file_stem` — implement: scan `path` for last `/`, then
  last `.` after it, slice between.
- `load_wav_from_memory`: same but no name set, no file read.
- `open`: read whole file into `file_data`; also do a **full** `wav_decode` up
  front purely to populate `channels/sample_rate/total_frames` (Rust discards the
  samples: `let (info, _) = ...`). Keep `file_offset=0`, `finished=0`.
- `read_chunk(chunk_frames)`:
  1. `remaining = total_frames - frames_read`; `frames_to_read = min(chunk_frames, remaining)`; if 0 return empty vec.
  2. `samples_needed = frames_to_read * channels`.
  3. While `pending.len() < samples_needed && !finished`:
     - if `file_offset < file_len`: take a **4096-byte** slice `[file_offset, min(+4096, file_len))`, advance offset, `feed` it, append every `Samples` event's data to `pending`, set `finished` on `End`, stash `Header` info.
     - else: `flush`, append any `Samples`, set `finished=1`.
  4. `take = min(samples_needed, pending.len())`; drain first `take` samples into
     result; `frames_read += take / channels`; return result vec.
- `read_all`: `read_chunk(total_frames)` then mono/stereo build.
- Every `?`/`map_err` path returns `NIDHI_ERR_IMPORT`.

---

## 2. `benches/benchmarks.rs` — parity benchmarks to reproduce

Rust uses **criterion** (`harness=false`, `[[bench]] name="benchmarks"`). Cyrius
has no criterion — mirror shravan's manual-timing harness in `src/bench.cyr`:
`time_now_ns()` via `clock_gettime(CLOCK_MONOTONIC)` (syscall 228, `bench.cyr:451`),
warm-up loop, then N iters between `t0=time_now_ns()`/`t1`, and `bench_report(t1-t0, iters)`
(pattern at `bench.cyr:505-540`). Use `black_box` equivalent = store result to a
volatile sink so the optimizer can't elide it.

### 2.1 Shared setup helpers (benchmarks.rs:15-45)

- `make_engine(max_voices, sample_rate) -> SamplerEngine`: builds a 1-second
  440 Hz sine **mono** sample (`frames = sample_rate`), adds it to a
  `SampleBank` (→ id), makes a `Zone` (`with_key_range(0,127)`,
  `with_root_note(60)`, `with_loop(LoopMode::Forward, 0, frames-1)`), one
  `Instrument("bench")`, then `SamplerEngine::new(max_voices, sample_rate)` +
  `set_bank` + `set_instrument`.
- `trigger_notes(engine, count)`: spreads `count` notes across MIDI 36..96:
  `step = count>1 ? 60/(count-1) : 0`; `note = min(36 + i*step, 96)`;
  `engine.note_on(note, 100)` for each.

### 2.2 Benchmark table (name → input → metric)

| # | Benchmark name (group/param) | What it measures | Input / setup | Metric |
|---|---|---|---|---|
| 1 | `voice_count_scaling/{1,4,8,16,32,64}` | per-call cost of one stereo sample vs active voice count | `make_engine(64, 44100)`, `trigger_notes(n)` | time per `engine.next_sample_stereo()` |
| 2 | `fill_buffer_stereo/{1,8,16}` | block render of a 512-frame (1024-sample) stereo buffer — typical audio callback | `make_engine(64,44100)`, `trigger_notes(n)`, `buf=1024 f32` | time per `buf.fill(0); engine.fill_buffer_stereo(&mut buf)` |
| 3 | `fill_buffer_per_sample/{1,8,16}` | per-sample baseline to compare vs block render | same as #2 | time to fill 1024 buf via loop of `next_sample_stereo()` writing `buf[i],buf[i+1]` |
| 4 | `interpolation_cubic_1k` | cubic Hermite interpolation throughput (mono) | 1 s 440 Hz mono sample (44100 frames) | time for 1000× `sample.read_cubic(pos)`, `pos=i*0.73+100.0`, summed |
| 5 | `interpolation_stereo_1k` | stereo interpolation throughput | 1 s 440 Hz **stereo** sample (44100 frames, 88200 samples) | time for 1000× `sample.read_stereo_interpolated(pos)`, same `pos`, L/R summed |
| 6 | `fill_buffer_stereo_filtered_8v` | filter processing cost in block render | mono sine sample; `Zone` with `with_filter(2000.0, 0.7)` + `with_filter_type(FilterMode::LowPass)`; `SamplerEngine::new(16,44100)`; `trigger_notes(8)`; `buf=1024` | time per `buf.fill(0); fill_buffer_stereo(&mut buf)` |
| 7 | `wsola_1sec_2x` | WSOLA time-stretch throughput | `TimeStretcher::new(data, 44100.0)` on 1 s 440 Hz mono | time per `ts.stretch(2.0)` |

Criterion `criterion_group!(benches, …)` order (benchmarks.rs:196): 1,2,3,4,5,6,7.
Public API these exercise (must exist in the Cyrius port):
`SamplerEngine::{new, set_bank, set_instrument, note_on, next_sample_stereo,
fill_buffer_stereo}`, `SampleBank::{new, add}`, `Sample::{from_mono, from_stereo,
read_cubic, read_stereo_interpolated}`, `Zone::{new, with_key_range,
with_root_note, with_loop, with_filter, with_filter_type}`,
`Instrument::{new, add_zone}`, `LoopMode::Forward`, `zone::FilterMode::LowPass`,
`stretch::TimeStretcher::{new, stretch}`. All exported via `nidhi::prelude::*`.

`read_cubic`/`read_stereo_interpolated` take `position: f64` (bit-pattern in
Cyrius) and return `f32` / `(f32,f32)` — return f64 bit-patterns in Cyrius.

---

## 3. `fuzz/fuzz_targets/` — fuzz harnesses

Rust uses **cargo-fuzz / libFuzzer** (`libfuzzer-sys 0.4`); `fuzz/Cargo.toml`
builds nidhi with `features=["std"]` only (**no `io` feature** — fuzzing is
parser-only, no WAV/shravan). Two targets:

| Target | File | Fuzzes | Invariant |
|---|---|---|---|
| `fuzz_sf2` | `fuzz_targets/fuzz_sf2.rs` | `nidhi::sf2::parse(data: &[u8])` on arbitrary bytes | parse must never panic on any input |
| `fuzz_sfz` | `fuzz_targets/fuzz_sfz.rs` | `nidhi::sfz::parse(input: &str)` on any UTF-8 (guarded by `from_utf8`), then on `Ok`: `sfz.to_zones(44100.0)` and `sfz.to_instrument("fuzz", 44100.0)` | parse **and** both conversions must never panic |

`fuzz_sf2.rs` body:
```rust
fuzz_target!(|data: &[u8]| { let _ = nidhi::sf2::parse(data); });
```
`fuzz_sfz.rs` body:
```rust
fuzz_target!(|data: &[u8]| {
    if let Ok(input) = core::str::from_utf8(data) {
        if let Ok(sfz) = nidhi::sfz::parse(input) {
            let _ = sfz.to_zones(44100.0);
            let _ = sfz.to_instrument("fuzz", 44100.0);
        }
    }
});
```

**Cyrius port note:** Cyrius has no libFuzzer/panics. Reproduce as a
"never-crash" driver reading bytes from stdin/argv and calling the ported
`sf2_parse(data,len)` / `sfz_parse(str,len)` + `sfz_to_zones` / `sfz_to_instrument`;
the parity invariant is "returns an error code, never a segfault/abort" for any
input. shravan's fuzz dir (`/home/macro/Repos/shravan/fuzz`) is the reference
pattern for how the Cyrius harnesses were built. Functions being fuzzed live in
the sf2/sfz port specs; the UTF-8 pre-check and both `to_zones`/`to_instrument`
calls with `44100.0` and name `"fuzz"` must be preserved for parity.

---

## 4. `Makefile` & roadmap context

**Makefile** (`/home/macro/Repos/nidhi/Makefile`) — `.PHONY` targets, all wrap
cargo:
- `check` = `fmt clippy test audit`
- `fmt` = `cargo fmt --all -- --check`
- `clippy` = `cargo clippy --all-features --all-targets -- -D warnings`
- `test` = `cargo test --all-features` **and** `cargo test --no-default-features`
- `audit` = `cargo audit`; `deny` = `cargo deny check`
- `bench` = `cargo bench`
- `coverage` = `cargo llvm-cov --all-features --html`
- `build` = `cargo build --release --all-features`
- `doc` = `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
- `clean` = `cargo clean` + rm coverage/

For Cyrius, replace with `cyrius` build/test/bench invocations mirroring
shravan's `cyrius.cyml` targets; keep the same logical phases (fmt/lint/test/
bench). The `--no-default-features` test leg matters: it exercises the `no_std`
(no naad, no shravan) path — i.e. **io is excluded** in that build.

**Roadmap** (`docs/development/roadmap.md`):
- v1.1.0 (shipped): zero-alloc render path, SIMD (`simd` feature, SSE2/NEON),
  filter-coeff caching, denormal flush, param smoothing. The benchmark suite in
  §2 is the v1.1.0 parity target ("voice count scaling, buffer fill throughput,
  interpolation cost").
- Backlog: shravan already replaced `hound` (1.0.1); **planned** FLAC/AIFF/OGG
  `io` helpers — shravan (Cyrius) already provides `flac_decode`, `aiff_decode`,
  `ogg` decode, so the Cyrius `io` module can expose them cheaply later. Not in
  scope for parity now; `io` currently = WAV only.

---

## 5. Feature-gating & build facts

- `Cargo.toml`: `io = ["std", "dep:shravan"]`; shravan pulled with
  `default-features=false, features=["wav","pcm","streaming"]`, version `1.0.1`.
  `std = ["dep:naad", …]`. `full = ["std","io","logging","simd"]`. Edition 2024,
  MSRV/`rust-version` 1.89, license `GPL-3.0-only`.
- `io.rs` uses `std::path::Path`, `std::fs::read` → Cyrius uses the syscall
  wrappers (`file_open`/`file_read`/`file_close`, seen in `shravan.cyr:1713`).
- **Samples are f64 bit-patterns end-to-end in Cyrius** (shravan decodes to f64);
  the Rust `Vec<f32>` boundary conversions are no-ops on the Cyrius side.
- shravan Cyrius perf reference (`benchmarks-rust-v-cyrius.md`): `wav_decode`
  ~42× slower than Rust, PCM conversion ~200× (f64 vs f32) — expect the same
  order for nidhi's WAV load path; parity is *correctness + relative shape*, not
  matching Rust's absolute ns.
