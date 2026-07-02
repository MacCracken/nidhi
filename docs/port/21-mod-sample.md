# Port Spec 21 — `sample.rs`, `capture.rs`, `instrument.rs`

Rust → Cyrius port brief. READ-ONLY recon; nothing was modified.

Cyrius model reminder: everything is `i64`; heap structs with untyped fields;
`#derive(accessors)` generates `Type_field` / `Type_set_field`; floats are `f64`
bit-patterns manipulated with `f64_add`/`f64_sub`/`f64_mul`/`f64_div`/`f64_lt`/… ;
errors are negative integer codes; NO serde / generics / trait-objects / SIMD.

**Critical porting decision — f32 vs f64.** The Rust sources are `f32`-heavy
(`Vec<f32>` sample data, `f32` energies/peaks). Cyrius has only `f64`. Port ALL
audio math as `f64`. This changes NOTHING about the algorithms, only precision.
Sample buffers become heap arrays of `f64` bit-patterns. Constants like `1e-10`,
`0.5`, `1e-10` must become f64 literals. The SIMD path (`cubic_hermite_stereo_sse`)
does NOT port — it is a bit-exact optimization of the scalar path; implement only
the scalar cubic Hermite and drop the `simd`/`x86_64` cfg entirely.

Source files:
- `/home/macro/Repos/nidhi/src/sample.rs` (494 lines)
- `/home/macro/Repos/nidhi/src/capture.rs` (326 lines)
- `/home/macro/Repos/nidhi/src/instrument.rs` (193 lines)
- Dependency read for context: `/home/macro/Repos/nidhi/src/zone.rs`, `/home/macro/Repos/nidhi/src/error.rs`

---

## 1. `sample.rs`

### 1.1 Public type `SampleId`

Rust:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[must_use]
pub struct SampleId(pub u32);
```
Newtype over `u32`. In Cyrius this is just a plain `i64` value (the index). No
struct needed — pass the raw integer. Equality is integer `==`. If a heap struct
is required for uniformity, make a 1-field struct `SampleId{ value }`. Values are
non-negative bank indices (see `SampleBank_add`). Recommend: **represent as bare
i64** and drop the wrapper. Error type `SampleNotFound(SampleId)` (see error.rs)
just carries this i64.

### 1.2 Public type `Sample`

Rust fields (all `pub(crate)`):
```rust
pub struct Sample {
    data: Vec<f32>,      // interleaved if stereo
    channels: u32,       // 1 = mono, 2 = stereo
    sample_rate: u32,    // Hz
    frames: usize,       // samples per channel
    name: String,
    slices: Vec<usize>,  // REX-style slice points (frame indices)
}
```

Cyrius struct with `#derive(accessors)`:
```
#derive(accessors)
struct Sample {
    data,         // heap array of f64 bit-patterns, length = frames*channels
    channels,     // i64: 1 or 2
    sample_rate,  // i64
    frames,       // i64
    name,         // heap string (or 0/null if unnamed)
    slices,       // heap array of i64 frame indices; length stored alongside or 0-len
}
```
Note: `data` holds INTERLEAVED f64. `frames = data_len / channels`. Both `data`
and `slices` are dynamic arrays — port with your array/vec primitive tracking a
length. `channels` is invariant 1 or 2 throughout.

#### Constructors / builders (public fns)

- `Sample::from_mono(data: Vec<f32>, sample_rate: u32) -> Sample`
  Sets `channels=1`, `frames=data.len()`, empty name, empty slices.
  Cyrius: `Sample_from_mono(data, len, sample_rate)` — frames = len.

- `Sample::from_stereo(data: Vec<f32>, sample_rate: u32) -> Sample`
  Sets `channels=2`, `frames = data.len() / 2`, empty name/slices.
  Cyrius: `Sample_from_stereo(data, len, sample_rate)` — frames = len / 2 (integer div).

- `Sample::with_name(self, name) -> Self` — builder, sets name, returns self.
- `Sample::with_slices(self, slices: Vec<usize>) -> Self` — builder, sets slices.

Cyrius has no move/consume builders; make these mutators or return the same
heap pointer after `Sample_set_name` / `Sample_set_slices`.

#### Accessors (all trivial, `#[inline] #[must_use]`)

- `slices(&self) -> &[usize]`  → `Sample_slices` (+ length)
- `data(&self) -> &[f32]`      → `Sample_data`
- `channels(&self) -> u32`     → `Sample_channels`
- `sample_rate(&self) -> u32`  → `Sample_sample_rate`
- `frames(&self) -> usize`     → `Sample_frames`
- `name(&self) -> &str`        → `Sample_name`

These map directly to generated `#derive(accessors)` getters.

#### Private frame readers (helpers)

`read_mono_frame(&self, idx: isize) -> f32`  (private):
```
if idx < 0 || idx as usize >= frames { return 0.0 }
if channels == 1 { data[idx] }
else { let ch = channels; (data[idx*ch] + data[idx*ch+1]) * 0.5 }
```
Note: uses `channels as usize` for the interleave stride but only ever reads
lanes 0 and 1 (so works for ch==2; for ch>2 would read first two, but channels
is always 1 or 2). Out-of-range (negative OR >= frames) returns 0.0 — this
zero-padding is essential for the interpolator at buffer edges.

`read_stereo_frame(&self, idx: isize) -> (f32, f32)`  (private):
```
if idx < 0 || idx as usize >= frames { return (0.0, 0.0) }
if channels == 1 { let v = data[idx]; (v, v) }        // mono duplicated to both
else { (data[idx*ch], data[idx*ch+1]) }
```
Cyrius: returns two values — either a 2-elem heap pair, out-params, or pack into
a tiny struct. Same zero-pad-on-OOB behavior.

#### Public interpolation fns (HOT PATH — mark inline-equivalent)

`cubic_hermite(y0,y1,y2,y3,t) -> f32`  (public, static, `#[must_use]`) —
**Catmull-Rom cubic Hermite**. EXACT formula, preserve operation order:
```
a = -0.5*y0 + 1.5*y1 - 1.5*y2 + 0.5*y3
b =  y0     - 2.5*y1 + 2.0*y2 - 0.5*y3
c = -0.5*y0            + 0.5*y2
d =  y1
return ((a*t + b)*t + c)*t + d          // Horner
```
Cyrius: pure f64 arithmetic via `f64_mul`/`f64_add`. Constants: -0.5, 1.5, -1.5,
0.5, -2.5, 2.0. This is the correctness anchor for the whole engine.

`read_cubic(&self, position: f64) -> f32`  (public, `#[must_use]`):
```
if frames == 0 { return 0.0 }
idx  = floor(position) as isize
frac = (position - idx) as f32                 // fractional part, 0..1
y0 = read_mono_frame(idx-1)
y1 = read_mono_frame(idx)
y2 = read_mono_frame(idx+1)
y3 = read_mono_frame(idx+2)
return cubic_hermite(y0,y1,y2,y3,frac)
```
Cyrius: `floor` via `f64_floor` (or truncate toward -inf — must be FLOOR not
trunc, matters for negative positions). `idx` is a signed i64.

`read_interpolated(&self, position: f64) -> f32`  (public) — thin alias, just
calls `read_cubic(position)`.

`read_stereo_interpolated(&self, position: f64) -> (f32, f32)`  (public):
```
if frames == 0 { return (0.0, 0.0) }
idx  = floor(position); frac = position - idx
(l0,r0)=read_stereo_frame(idx-1); (l1,r1)=read_stereo_frame(idx)
(l2,r2)=read_stereo_frame(idx+1); (l3,r3)=read_stereo_frame(idx+2)
left  = cubic_hermite(l0,l1,l2,l3,frac)
right = cubic_hermite(r0,r1,r2,r3,frac)
return (left, right)
```
The `simd` cfg branch calls `cubic_hermite_stereo_sse` producing bit-identical
results — **ignore it in Cyrius, port only the scalar branch above.**

`cubic_hermite_stereo_sse(...)` — SSE-only private helper; **DO NOT PORT.**

#### DSP: `detect_onsets(&mut self, threshold: f32, min_slice_frames: usize)`

Energy-based transient/onset detection. Mutates `self.slices`. Algorithm EXACT:
```
slices.clear()
if frames < 2 { return }
window = clamp(512, min = frames/2, ...); precisely: window = max( min(512, frames/2), 1 )
hop    = max(window/2, 1)
threshold = clamp(threshold, 0.01, 1.0)

// 1. Per-window mean energy (list of (start_frame, energy))
energies = []
pos = 0
while pos + window <= frames:
    energy = 0.0
    for i in pos .. pos+window:
        s = (channels==1) ? data[i]
                          : (data[i*2] + data[i*2+1]) * 0.5   // mono-mix stereo
        energy += s*s
    energies.push( (pos, energy / window) )    // MEAN energy over window
    pos += hop

if energies.len() < 2 { return }

// 2. Peak-energy normalization factor
max_energy = max over energies of e
if max_energy < 1e-10 { return }               // silence guard

// 3. Onset = normalized positive energy jump vs previous window
last_slice = 0
for i in 1 .. energies.len():
    (frame, energy) = energies[i]
    prev_energy     = energies[i-1].1
    diff = (energy - prev_energy) / max_energy
    if diff > threshold  AND  frame.saturating_sub(last_slice) >= min_slice_frames:
        slices.push(frame)
        last_slice = frame
```
Key details for parity:
- `window = max(min(512, frames/2), 1)`; `hop = max(window/2, 1)`. Both guards
  exist to avoid `hop==0` infinite loop on tiny samples (regression test
  `detect_onsets_very_short_sample`). Preserve exactly.
- Energy is stored as MEAN (sum-of-squares / window), NOT sum.
- Stereo mixed to mono via `(L+R)*0.5` before squaring.
- `diff` is the normalized ONSET (energy increase); negative diffs never trigger.
- `frame.saturating_sub(last_slice)` — Rust saturating subtract; since `frame`
  monotonically increases and `last_slice <= frame`, plain `frame - last_slice`
  is safe in Cyrius (never underflows here), but keep >= 0 semantics.
- Comparison is strict `>` threshold.

### 1.3 Public type `SampleBank`

Rust:
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SampleBank { samples: Vec<Sample> }
```
Cyrius:
```
#derive(accessors)
struct SampleBank { samples }   // heap array of Sample pointers + length
```
Fns:
- `new() -> SampleBank` — empty vec.
- `add(&mut self, sample: Sample) -> SampleId`
  ```
  id = SampleId(samples.len() as u32)   // id = current length BEFORE push
  samples.push(sample)
  return id
  ```
  Cyrius: `id = samples_len; samples[samples_len++] = sample; return id`.
- `get(&self, id: SampleId) -> Option<&Sample>` — bounds-checked `samples.get(id)`.
  Cyrius: return sample pointer or 0/negative if `id < 0 || id >= len`.
- `len(&self) -> usize` — `samples.len()`.
- `is_empty(&self) -> bool` — `len == 0`.

### 1.4 Inline tests in `sample.rs` (asserts to preserve as Cyrius test fns)

- `sample_from_mono`: mono 100×0.5 @44100 named "test" → channels==1, frames==100, name=="test".
- `sample_interpolation`: ramp [0,0.25,0.5,0.75,1.0]: `read_interpolated(2.0)≈0.5`,
  `read_interpolated(2.5)≈0.625` (±0.01). Spike [0,0,1,0,0]: `read_interpolated(2.0)≈1.0`.
- `cubic_hermite_smooth`: `cubic_hermite(0,1,2,3, 0.5) ≈ 1.5` (±0.01) — linear ramp exact.
- `read_cubic_basic`: spike [0,0,1,0,0]: `read_cubic(2.0) ≈ 1.0`.
- `read_stereo_interpolated_basic`: interleaved L=1,R=0 repeated → at 1.5: l≈1.0, r≈0.0.
- `read_stereo_interpolated_mono_duplicates`: mono 0.5 → l≈0.5, r≈0.5.
- `bank_add_get`: add one sample → `get(id).is_some()`, `len()==1`.
- `detect_onsets_finds_transients`: 8000-frame silence with 0.9 burst in frames
  4000..4500; `detect_onsets(0.1, 256)` → non-empty; first slice in 3500..=5000.
- `detect_onsets_empty_sample`: empty → no slices.
- `detect_onsets_silence`: 4000 zeros → no slices (max_energy guard).
- `detect_onsets_very_short_sample`: 2-frame and 3-frame samples with
  `detect_onsets(0.1, 1)` must TERMINATE (no infinite loop). Critical regression.
- `manual_slices`: `with_slices([100,500,800])` → `slices()==[100,500,800]`.

---

## 2. `capture.rs`

Depends only on `crate::sample::Sample`. Note: mutates `Sample.data` and
`Sample.frames` DIRECTLY via `pub(crate)` field access (not accessors) — in
Cyrius use `Sample_set_data` / `Sample_set_frames`.

### 2.1 Public type `SampleRecorder`

Rust:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecorder {
    buffer: Vec<f32>,
    sample_rate: u32,
    channels: u32,
}
```
Cyrius:
```
#derive(accessors)
struct SampleRecorder { buffer, buffer_len, sample_rate, channels }
```
Fns:
- `new(sample_rate: u32, channels: u32) -> SampleRecorder`
  `channels = channels.clamp(1, 2)`. Empty buffer.
- `write(&mut self, data: &[f32])` — append/extend buffer with data (interleaved
  for stereo). Cyrius: append `data[0..len]` to `buffer`.
- `frames(&self) -> usize` — `buffer.len() / channels`.
- `clear(&mut self)` — truncate buffer to 0.
- `finish(self) -> Sample`
  `if channels == 2 { Sample::from_stereo(buffer, sr) } else { Sample::from_mono(buffer, sr) }`.
  Consumes buffer (moves into Sample). Cyrius: transfer the buffer pointer.
- `finish_processed(self, silence_threshold: f32) -> Sample`
  ```
  sample = self.finish()
  trim_silence(&mut sample, silence_threshold)
  normalize_peak(&mut sample)
  return sample
  ```

### 2.2 Free fn `trim_silence(sample: &mut Sample, threshold: f32)`

Remove leading + trailing silence. Algorithm EXACT:
```
threshold = max(threshold, 0.0)
ch = channels; frames = frames
if frames == 0 { return }

// first non-silent frame: any channel |data[f*ch+c]| > threshold
start = first f in 0..frames where exists c in 0..ch: |data[f*ch+c]| > threshold
        else frames                        // (unwrap_or(frames) → all silent)

// last non-silent frame + 1
end = (last f in 0..frames, scanning reversed, where any channel > threshold)
      mapped to f+1, else 0                // (unwrap_or(0))

if start >= end {                          // fully silent
    data.clear(); frames = 0; return
}

sample_start = start * ch
sample_end   = end   * ch
data   = data[sample_start .. sample_end]  // copy sub-slice
frames = end - start
```
Details: comparison is strict `>` (frames exactly == threshold count as silent).
`start` counts from front, `end` scans from back. If all silent, `start==frames`
and `end==0` so `start>=end` → clears. Cyrius: build a new f64 array of the
retained slice and set it plus new frames.

### 2.3 Free fn `normalize_peak(sample: &mut Sample)`

Scale so loudest |sample| → 1.0 (0 dBFS):
```
peak = max over all data of |s|          // fold over abs values, start 0.0
if peak > 1e-10 {
    gain = 1.0 / peak
    for s in data: s *= gain
}
```
No-op if silent (peak ≤ 1e-10). Cyrius: `f64_abs`, running max, then scale.

### 2.4 Free fn `normalize_rms(sample: &mut Sample, target_rms: f32)`

Scale to target RMS:
```
if data.is_empty() { return }
rms = sqrt( sum(s*s for s in data) / data.len() )   // over ALL samples (interleaved)
if rms > 1e-10 {
    gain = target_rms / rms
    for s in data: s *= gain
}
```
`target_rms` typically 0.1–0.3. RMS computed over the full interleaved buffer
(NOT per-channel). Cyrius: `f64_sqrt`.

### 2.5 DSP: `detect_loop_points(sample: &Sample, min_loop_frames: usize) -> Vec<(usize,usize)>`

Find candidate `(start,end)` loop frame pairs, best-first, via zero-crossing +
normalized cross-correlation. `#[must_use]`. Algorithm EXACT:
```
data = data(); frames; ch = channels
if frames < min_loop_frames * 2 { return [] }

// 1. Downmix to mono: mono[f] = (sum over channels data[f*ch+c]) / ch
mono = [ (Σ_c data[f*ch+c]) / ch  for f in 0..frames ]

// 2. Positive-going zero crossings: mono[i-1] <= 0.0 && mono[i] > 0.0
crossings = [ i for i in 1..mono.len() if mono[i-1] <= 0.0 && mono[i] > 0.0 ]
if crossings.len() < 2 { return [] }

// 3. Score pairs by boundary similarity
compare_len = min(64, frames/4)
candidates: Vec<(start,end,score:f64)> = []
for (i, start) in crossings.enumerate():
    for end in crossings[i+1 ..]:
        if end - start < min_loop_frames { continue }
        if start + compare_len > frames || end + compare_len > frames { continue }
        // normalized cross-correlation (cosine similarity) over compare_len samples
        dot=0; norm_a=0; norm_b=0
        for k in 0..compare_len:
            a = mono[start+k]; b = mono[end+k]
            dot += a*b; norm_a += a*a; norm_b += b*b
        denom = sqrt(norm_a * norm_b)
        score = (denom > 1e-10) ? dot/denom : 0.0
        candidates.push((start,end,score))
    if candidates.len() > 100 { break }     // outer-loop cap for speed

// 4. Sort by score DESCENDING (NaN-safe: treat uncomparable as Equal)
candidates.sort_by(|a,b| b.score.partial_cmp(&a.score).unwrap_or(Equal))

// 5. Return top 10 (start,end), dropping scores
return candidates.take(10).map(|(s,e,_)| (s,e))
```
Parity notes:
- All correlation math is f64 already in the Rust source — direct port.
- `mono` downmix divides by `ch` (channel count), producing a mean.
- Zero-crossing predicate is `<= 0` then `> 0` (positive-going only).
- The `candidates.len() > 100` break is checked ONCE per OUTER iteration (after
  the inner `end` loop finishes), so the actual count can exceed 100 — do not
  move it inside the inner loop.
- `compare_len = min(64, frames/4)` — can be 0 for very short samples, in which
  case the inner correlation loop runs 0 times, score = (denom>1e-10?…:0.0)=0.0.
- Sort is DESCENDING by score; Cyrius needs a stable-enough sort; ties keep any
  order (Rust `sort_by` is stable). Return at most 10 pairs.
- Return type is a heap array of (i64,i64) pairs.

### 2.6 Inline tests in `capture.rs`

- `recorder_basic`: mono, write [0.1,0.2,0.3] then [0.4,0.5] → frames==5;
  finish → frames==5, channels==1.
- `recorder_stereo`: stereo, write [0.1,0.2,0.3,0.4] → frames==2; finish → frames==2, channels==2.
- `trim_silence_removes_padding`: [0,0,0,0.5,0.8,0.3,0,0], trim(0.01) → frames==3,
  data[0]≈0.5.
- `trim_silence_all_silent`: 100 zeros, trim(0.01) → frames==0.
- `normalize_peak_scales_to_one`: [0,0.25,-0.5,0.1] → peak becomes 1.0 (±0.001).
- `normalize_rms_adjusts_level`: 100×0.5, target 0.2 → measured RMS ≈0.2 (±0.01).
- `detect_loop_points_returns_candidates`: 4410-sample 100 Hz sine @44100,
  `detect_loop_points(_,100)` → non-empty; loops[0]=(start,end) with end>start,
  end-start>=100.
- `detect_loop_points_short_sample`: 10 zeros, min 100 → empty (frames < 2*min).
- `finish_processed_trims_and_normalizes`: 100 silence + 200×0.25 + 100 silence,
  `finish_processed(0.01)` → frames==200, peak≈1.0 (±0.01).

---

## 3. `instrument.rs`

Depends on `crate::zone::Zone` (see §4). An Instrument owns a `Vec<Zone>` plus
per-group round-robin counters.

### 3.1 Public type `Instrument`

Rust:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Instrument {
    name: String,
    zones: Vec<Zone>,       // ordered by key range for lookup
    rr_counters: Vec<u32>,  // round-robin counter per group index
}
```
Cyrius:
```
#derive(accessors)
struct Instrument { name, zones, zones_len, rr_counters, rr_counters_len }
```

### 3.2 Public fns

- `new(name) -> Instrument` — empty zones, empty rr_counters.

- `add_zone(&mut self, zone: Zone)`
  ```
  group = zone.group()
  zones.push(zone)
  if group > 0 {
      needed = group + 1
      if rr_counters.len() < needed { rr_counters.resize(needed, 0) }
  }
  ```
  `rr_counters` is indexed BY GROUP NUMBER, so it must be at least `group+1`
  long. `resize(n, 0)` extends with zeros. group 0 never allocates a counter.

- `find_zones(&self, note: u8, velocity: u8) -> Vec<&Zone>`  (`#[must_use]`)
  Return all zones where `zone.matches(note, velocity)` (borrowed refs).
  Cyrius: heap array of zone pointers.

- `find_zone_rr(&mut self, note: u8, velocity: u8) -> Option<(usize, &Zone)>`
  Round-robin voice selection. Algorithm EXACT:
  ```
  matching = [ i for (i,z) in zones.enumerate() if z.matches(note,velocity) ]
  if matching.is_empty() { return None }

  first_match = matching[0]
  group = zones[first_match].group()

  if group == 0 {                         // ungrouped → always first match
      return Some((first_match, &zones[first_match]))
  }

  // grouped: keep only matches in the SAME group as first_match
  group_matches = [ i in matching if zones[i].group() == group ]
  if group_matches.is_empty() { return None }   // (defensive; can't happen since first_match qualifies)

  needed = group + 1
  if rr_counters.len() < needed { rr_counters.resize(needed, 0) }

  counter = &mut rr_counters[group]
  pick = (counter as usize) % group_matches.len()
  counter = counter.wrapping_add(1)        // post-increment, wrapping u32

  zone_idx = group_matches[pick]
  return Some((zone_idx, &zones[zone_idx]))
  ```
  Parity notes:
  - The group used for RR is the group of the FIRST matching zone. If several
    groups match a note, only the first-matched group participates in RR; other
    groups are ignored for that call.
  - Counter is read → modulo → then post-incremented with WRAPPING add (u32
    wrap at 2^32). In Cyrius keep the counter masked to 32 bits
    (`counter = (counter + 1) & 0xFFFFFFFF`) to reproduce wrap, and `% len`.
  - Returns 0-based zone INDEX plus the zone reference.
  - Cyrius `Option<(usize,&Zone)>` → return a sentinel (e.g. -1 index / 0 ptr)
    for None, or out-params.

- `name(&self) -> &str` — `Instrument_name`.
- `zone_count(&self) -> usize` — `zones.len()`.
- `zones(&self) -> &[Zone]` — `Instrument_zones` (+ len).

### 3.3 Inline tests in `instrument.rs`

- `instrument_find_zones`: two zones (SampleId 0 key 60–72, SampleId 1 key 48–59).
  `find_zones(66,100)` → 1 zone, sample_id==0. `find_zones(50,100)` → 1 zone, sample_id==1.
- `round_robin_cycles`: three zones all key 60–72, group 1, SampleId 0/1/2.
  `find_zone_rr(66,100)` four times → indices/ids 0,1,2 then wraps to 0.
- `round_robin_ungrouped_returns_first`: two zones key 60–72, group 0 (default).
  Two `find_zone_rr` calls return the SAME index (always first).
- `round_robin_no_match`: one zone key 60–72 group 1; `find_zone_rr(50,100)` → None.

---

## 4. Dependency: `Zone` (from `zone.rs`) — only the parts instrument.rs needs

`Zone` is a large `pub(crate)`-field struct (SampleId, key_lo/hi, vel_lo/hi,
root_note, tune_cents, volume_db, pan, loop fields, filter fields, group,
choke_group, vel_curve, adsr/fileg options, LFO fields, time_stretch,
output_bus). Full port is a separate spec. Methods USED here:

```rust
pub fn new(sample_id: SampleId) -> Self          // defaults: key 0..127, vel 1..127,
                                                 // root_note 60, group 0, loop OneShot, ...
pub fn with_key_range(self, lo: u8, hi: u8) -> Self
pub fn with_group(self, group: u32) -> Self
pub fn group(&self) -> u32                        // 0 = ungrouped
pub fn sample_id(&self) -> SampleId
pub fn matches(&self, note: u8, velocity: u8) -> bool {
    note >= key_lo && note <= key_hi && velocity >= vel_lo && velocity <= vel_hi
}
```
`matches` is an inclusive key AND velocity range test — port as four integer
comparisons. Default zone matches key 0..=127, vel 1..=127.

---

## 5. Error / dependency summary for these three files

- `sample.rs`: deps `alloc::{String,Vec}`, `serde`. No error returns — all
  infallible (returns, Options). SIMD path behind `simd`+`x86_64` cfg (SKIP).
- `capture.rs`: deps `alloc::Vec`, `serde`, `crate::sample::Sample`. Mutates
  Sample's `data`/`frames` fields directly. No error returns. Free functions
  (not methods): `trim_silence`, `normalize_peak`, `normalize_rms`,
  `detect_loop_points`.
- `instrument.rs`: deps `alloc::{String,Vec}`, `serde`, `crate::zone::Zone`.
  No error returns. RR counters are `Vec<u32>` with wrapping increment.
- `error.rs` (context): `NidhiError` enum, `SampleNotFound(SampleId)` variant is
  the only one touching these types; in Cyrius, a negative integer code carrying
  the sample id. None of the three files under spec return `Result`.

## 6. Cyrius porting cautions (parity-critical)

1. **f32→f64 everywhere.** Every literal (0.5, 1.5, 1e-10, 0.01, etc.) becomes
   f64. Tolerances in tests (±0.01, ±0.001) remain valid or tighten.
2. **Floor, not truncate**, for `position.floor()` in interpolation (matters at
   negative positions used for edge zero-padding via idx-1).
3. **Zero-pad OOB reads** in `read_mono_frame`/`read_stereo_frame` — do not clamp
   to nearest sample; return 0.0. This shapes the interpolated tails.
4. **`detect_onsets` guards** `window=max(min(512,frames/2),1)`,
   `hop=max(window/2,1)` prevent an infinite loop — never simplify away.
5. **Mean vs sum energy** — onset energy is `sum_sq / window` (mean).
6. **RMS over full interleaved buffer**, loop-point downmix divides by `ch`.
7. **`detect_loop_points` 100-candidate cap** is per-outer-iteration only.
8. **RR counter** is post-increment with u32 wrap; index by group number, size
   `rr_counters` to `max_group+1`, group 0 uses NO counter (returns first match).
