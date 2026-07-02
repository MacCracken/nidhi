# Port Spec: `src/sf2.rs` — SF2/SoundFont 2 Binary Parser → Cyrius

Source: `/home/macro/Repos/nidhi/src/sf2.rs` (30 KB, 899 lines). Read-only recon; nothing modified.

This module parses a RIFF-based SoundFont-2 file from an in-memory byte buffer (no file I/O)
and produces nidhi-native structures. **All multi-byte integers are little-endian.** Precision
of every byte offset and record size below is load-bearing — reproduce exactly.

---

## 1. Public surface

### Public type
```rust
#[non_exhaustive]
pub struct Sf2Preset {
    pub name: String,        // preset name (max 20 bytes, NUL-trimmed, UTF-8 lossy)
    pub bank: u16,           // MIDI bank number
    pub preset_number: u16,  // MIDI program number
}
```
Cyrius: heap struct with 3 fields `name` (heap string ptr as i64), `bank` (i64), `preset_number`
(i64). Add `#derive(accessors)` → `Sf2Preset_name`, `Sf2Preset_bank`, `Sf2Preset_preset_number`
plus `_set_` variants. No serde in Cyrius — drop the Serialize/Deserialize derives.

### Public function (the only public entry point)
```rust
pub fn parse(data: &[u8]) -> Result<(Vec<Sf2Preset>, Vec<Instrument>, SampleBank)>
```
Rust returns a 3-tuple. Cyrius has no tuples/generics — return a small heap "result" struct
holding three parallel list pointers: `presets` (list of Sf2Preset), `instruments` (list of
Instrument), `bank` (SampleBank). On error return a **negative i64 error code** (see §7).
`presets[i]` and `instruments[i]` are parallel (same index). Instruments with zero zones are
skipped entirely (both vectors), so lengths stay in lock-step.

---

## 2. Internal (private) record structs

These mirror on-disk SF2 records after parsing. In Cyrius make each a heap struct; fields are i64.

| Struct | Fields (Rust types) |
|---|---|
| `PhdrRecord` | `name: String`, `preset: u16`, `bank: u16`, `bag_index: u16` |
| `BagRecord`  | `gen_index: u16` (only field kept; mod index is skipped) |
| `GenRecord`  | `oper: u16`, `amount: i16` |
| `InstRecord` | `name: String` (dead-code, kept), `bag_index: u16` |
| `ShdrRecord` | `name: String`, `start: u32`, `end: u32`, `loop_start: u32`, `loop_end: u32`, `sample_rate: u32`, `original_pitch: u8`, `sample_type: u16` |

`GenRecord` has a helper:
```rust
fn amount_range(&self) -> (u8, u8) {
    let lo = (self.amount & 0xFF) as u8;
    let hi = ((self.amount >> 8) & 0xFF) as u8;
    (lo, hi)
}
```
i.e. a range generator packs `lo` in the low byte, `hi` in the high byte of the i16 amount.
Cyrius: `GenRecord_amount_range_lo = amount & 0xFF`, `..._hi = (amount >> 8) & 0xFF`.
**Note the `amount` was read as `i16`** — for range gens treat as raw 16 bits; masking `& 0xFF`
and `>> 8` on a sign-extended i64 works if you first mask `amount` to 16 bits (`amount & 0xFFFF`)
before shifting, to avoid sign bleed. In Rust the `>> 8` is on `i16` (arithmetic) but then `& 0xFF`
discards the sign — so `hi = (amount as u16 >> 8) & 0xFF` is the safe Cyrius equivalent.

---

## 3. FourCC / magic constants

```
RIFF_ID = "RIFF"   SFBK_ID = "sfbk"   LIST_ID = "LIST"
```
pdta sub-chunk IDs matched by the parser:
```
PHDR_ID="phdr" PBAG_ID="pbag" PGEN_ID="pgen"
INST_ID="inst" IBAG_ID="ibag" IGEN_ID="igen" SHDR_ID="shdr"
```
Also matched as raw byte literals inside `parse`: `b"sdta"`, `b"smpl"`, `b"pdta"`, `b"INFO"`.
**`pmod`, `imod` and INFO sub-chunks are read past but ignored.** A FourCC is 4 raw bytes;
in Cyrius compare 4 bytes or pack into an i64 (`b0 | b1<<8 | b2<<16 | b3<<24`) and compare ints.

### Generator operator numbers (the only 6 handled)
```
GEN_INSTRUMENT           = 41   // preset-level: index into inst[]
GEN_KEY_RANGE            = 43   // range gen (lo,hi)
GEN_VEL_RANGE            = 44   // range gen (lo,hi)
GEN_SAMPLE_ID            = 53   // instrument-level: index into shdr[]
GEN_SAMPLE_MODES         = 54   // loop mode bits
GEN_OVERRIDING_ROOT_KEY  = 58   // root key override
```
All other generator opers are ignored (`_ => {}`).

---

## 4. Low-level byte-reading helpers (stdlib primitives to provide in Cyrius)

Every read is bounds-checked and returns an error (never panics/reads OOB).

| Rust fn | Behaviour | Cyrius stdlib helper needed |
|---|---|---|
| `read_u8(data, off)` | 1 byte → u8 | `bytes_get_u8(buf, off) -> i64` (err if `off >= len`) |
| `read_u16_le(data, off)` | LE u16; err if `off+2 > len` | `bytes_get_u16_le(buf, off) -> i64` |
| `read_i16_le(data, off)` | reads u16 LE then casts to i16 | `bytes_get_i16_le` = read u16 then sign-extend from bit 15 |
| `read_u32_le(data, off)` | LE u32; err if `off+4 > len` | `bytes_get_u32_le(buf, off) -> i64` |
| `read_fourcc(data, off)` | copies 4 bytes into `[u8;4]`; err if `off+4>len` | `bytes_get_fourcc(buf, off) -> i64` (pack 4 bytes LE) |
| `read_fixed_string(data, off, len)` | slice `[off..off+len]`, cut at first NUL byte (`unwrap_or(len)` if none), `String::from_utf8_lossy` | `bytes_read_cstr_fixed(buf, off, len) -> strptr` |

Bounds-error message format (all): `"unexpected end of data at offset {offset}"` → maps to a
single generic "truncated data" error code in Cyrius (see §7).

Endianness helpers exhaust the spec — **there is no big-endian anywhere**. `i16` sign handling:
`read_i16_le` reads the u16 then reinterprets; in Cyrius, if the u16 value `v >= 0x8000`, use
`v - 0x10000` to get the signed i64.

---

## 5. RIFF chunk iteration

```rust
struct Chunk<'a>   { id: [u8;4], data: &'a [u8] }
struct ChunkIter<'a>{ data: &'a [u8], offset: usize }
fn iter_chunks(data) -> ChunkIter
```
Iterator `next()` logic (reproduce exactly):
1. If `offset + 8 > len` → stop (return None). A chunk header is **8 bytes**: 4-byte id + 4-byte
   LE u32 size.
2. `id  = read_fourcc(data, offset)`.
3. `size = read_u32_le(data, offset+4) as usize`.
4. `data_start = offset + 8`.
5. `data_end = data_start.checked_add(size)`; on overflow → error
   `"chunk size overflow at offset {offset}"`, and set offset=len to stop.
6. If `data_end > len` → error `"chunk extends beyond data at offset {offset}"`, offset=len.
7. Yield chunk `{id, data: data[data_start..data_end]}`.
8. **Advance with even-padding:** `offset = data_end + (size & 1)` (RIFF chunks are word-aligned;
   odd-sized chunks have a trailing pad byte). Uses `saturating_add`.

Cyrius: implement as an explicit loop with a mutable cursor. `size & 1` is the pad. The overflow
check (step 5) matters only if size is near u32::MAX; in an i64 world `data_start + size` won't
overflow i64, but still guard `data_end > len`.

---

## 6. Record parsers (fixed-size record arrays)

Each pdta sub-chunk is a packed array of fixed-size records. `count = chunk.len() / SIZE`
(**integer division — trailing partial bytes ignored**). Loop `i in 0..count`, `off = i*SIZE`.

### `parse_phdr_records` — SIZE = **38** bytes (phdr record)
| Field | Offset | Size | Reader |
|---|---|---|---|
| name | 0 | 20 | fixed string |
| preset | 20 | 2 | u16 LE |
| bank | 22 | 2 | u16 LE |
| bag_index (wPresetBagNdx) | 24 | 2 | u16 LE |
| (library) | 26 | 4 | *skipped* |
| (genre) | 30 | 4 | *skipped* |
| (morphology) | 34 | 4 | *skipped* |

### `parse_bag_records` — SIZE = **4** bytes (pbag & ibag share this)
| Field | Offset | Size | Reader |
|---|---|---|---|
| gen_index (wGenNdx) | 0 | 2 | u16 LE |
| (mod_index) | 2 | 2 | *skipped* |

### `parse_gen_records` — SIZE = **4** bytes (pgen & igen share this)
| Field | Offset | Size | Reader |
|---|---|---|---|
| oper (sfGenOper) | 0 | 2 | u16 LE |
| amount (genAmount) | 2 | 2 | **i16 LE** |

### `parse_inst_records` — SIZE = **22** bytes
| Field | Offset | Size | Reader |
|---|---|---|---|
| name | 0 | 20 | fixed string |
| bag_index (wInstBagNdx) | 20 | 2 | u16 LE |

### `parse_shdr_records` — SIZE = **46** bytes
| Field | Offset | Size | Reader |
|---|---|---|---|
| name | 0 | 20 | fixed string |
| start (dwStart) | 20 | 4 | u32 LE |
| end (dwEnd) | 24 | 4 | u32 LE |
| loop_start (dwStartloop) | 28 | 4 | u32 LE |
| loop_end (dwEndloop) | 32 | 4 | u32 LE |
| sample_rate (dwSampleRate) | 36 | 4 | u32 LE |
| original_pitch (byOriginalPitch) | 40 | 1 | u8 |
| (byPitchCorrection) | 41 | 1 | *skipped* |
| (wSampleLink) | 42 | 2 | *skipped* |
| sample_type (sfSampleType) | 44 | 2 | u16 LE |

Every record array has a trailing **terminal/sentinel record** (EOP/EOI/EOS) — the parser
iterates `0..count.saturating_sub(1)` for phdr and uses the `[i+1]` sentinel's index as the
end bound; likewise inst uses `insts[ii+1].bag_index`. Do **not** drop the sentinel from storage.

---

## 7. Error handling → Cyrius integer error codes

Rust uses `NidhiError` enum; every failure here is `NidhiError::ImportError(String)`
(`Result<T> = core::result::Result<T, NidhiError>`). In Cyrius, `parse` returns a negative i64.
Suggested code map (all import-class errors; distinct codes optional but recommended):

| Condition | Rust message | Cyrius code (suggest) |
|---|---|---|
| `data.len() < 12` | "file too small to be a valid SF2" | `-1` |
| first FourCC != "RIFF" | "not a RIFF file" | `-2` |
| form type != "sfbk" | "RIFF form type is {..}, expected 'sfbk'" | `-3` |
| any truncated read | "unexpected end of data at offset {n}" | `-4` |
| chunk size overflow | "chunk size overflow at offset {n}" | `-5` |
| chunk extends beyond data | "chunk extends beyond data at offset {n}" | `-6` |
| missing sdta/smpl | "missing sdta/smpl chunk" | `-7` |
| missing pdta | "missing pdta chunk" | `-8` |
| missing phdr/pbag/pgen/inst/ibag/igen/shdr | "missing {name}" | `-9..-15` |

Zero-length / bad *content* inside otherwise-valid structure is **not** an error — the parser
silently skips (e.g. missing sample_id gen → `continue`; instrument index past end → `continue`;
ROM samples → `continue`; zero zones → preset dropped). Preserve this lenient behaviour.

---

## 8. `parse` control flow (the algorithm)

1. **Header validation** (§7 rows 1-3): need ≥12 bytes; `data[0..4]=="RIFF"`; `data[8..12]=="sfbk"`.
   Bytes `4..8` are the RIFF size — **read but not validated**.
2. **Top-level scan** over `iter_chunks(&data[12..])` (start after the 12-byte RIFF+form header).
   For each `LIST` chunk with `data.len() >= 4`, read its 4-byte list type at offset 0:
   - `"sdta"`: iterate its sub-chunks from `chunk.data[4..]`; capture the `"smpl"` sub-chunk's data
     into `sdta_smpl`. (24-bit `sm24` extension is ignored — only 16-bit PCM.)
   - `"pdta"`: store `chunk.data[4..]` (everything after the list-type FourCC) as `pdta`.
   - other list types (e.g. INFO): ignored.
   Missing smpl → err -7; missing pdta → err -8.
3. **pdta sub-chunk scan** over `iter_chunks(pdta_data)`: capture raw slices for phdr, pbag, pgen,
   inst, ibag, igen, shdr by FourCC match. `pmod`/`imod` ignored. Each missing → err -9..-15.
4. **Parse each record array** with the §6 parsers.
5. **Resolve presets → instruments → zones** (nested loop, below).

### Nested resolution loop (the heart)
```
for pi in 0 .. phdrs.len()-1:                       # skip terminal phdr (saturating_sub)
    phdr = phdrs[pi]
    bag_start = phdr.bag_index
    bag_end   = phdrs[pi+1].bag_index               # sentinel gives end
    inst_obj  = Instrument::new(phdr.name)

    for bi in bag_start .. bag_end:                 # preset bags
        if bi >= pbags.len(): break
        gen_start = pbags[bi].gen_index
        gen_end   = pbags[bi+1].gen_index  if bi+1 < pbags.len() else pgens.len()

        inst_index = None; preset_key_range=None; preset_vel_range=None
        for pg in pgens[gen_start .. min(gen_end, pgens.len())]:
            oper 41 -> inst_index = pg.amount as usize
            oper 43 -> preset_key_range = pg.amount_range()
            oper 44 -> preset_vel_range = pg.amount_range()
        if inst_index is None: continue
        ii = inst_index; if ii >= insts.len()-1: continue    # saturating_sub

        inst_rec = insts[ii]
        ibag_start = inst_rec.bag_index
        ibag_end   = insts[ii+1].bag_index          # sentinel

        for ib in ibag_start .. ibag_end:           # instrument bags
            if ib >= ibags.len(): break
            igen_start = ibags[ib].gen_index
            igen_end   = ibags[ib+1].gen_index if ib+1 < ibags.len() else igens.len()

            sample_id=None; key_range=(0,127); vel_range=(0,127)
            root_key_override=None; sample_modes=0
            for ig in igens[igen_start .. min(igen_end, igens.len())]:
                oper 53 -> sample_id = ig.amount as usize
                oper 43 -> key_range = ig.amount_range()
                oper 44 -> vel_range = ig.amount_range()
                oper 58 -> k = ig.amount as u8; if k<=127: root_key_override=k
                oper 54 -> sample_modes = ig.amount as u16
            if sample_id is None: continue
            sid = sample_id; if sid >= shdrs.len()-1: continue   # saturating_sub
            shdr = shdrs[sid]
            if shdr.sample_type & 0x8000 != 0: continue          # skip ROM samples

            root_key = root_key_override ?? shdr.original_pitch
            loop_mode = match (sample_modes & 3):
                0 -> OneShot ; 1|2 -> Forward ; 3 -> LoopSustain ; _ -> OneShot

            # preset range CLAMPS instrument range (intersection):
            final_key = preset_key_range ? (max(key.lo,pk.lo), min(key.hi,pk.hi)) : key_range
            final_vel = preset_vel_range ? (max(vel.lo,pv.lo), min(vel.hi,pv.hi)) : vel_range

            pcm = pcm16_to_f32(smpl_data, shdr.start, shdr.end)
            sample = Sample::from_mono(pcm, shdr.sample_rate).with_name(shdr.name)
            sample_bank_id = bank.add(sample)                    # SampleId(u32), sequential

            zone = Zone::new(sample_bank_id)
                     .with_key_range(final_key.0, final_key.1)
                     .with_vel_range(final_vel.0, final_vel.1)
                     .with_root_note(root_key)
            if loop_mode != OneShot:
                ls = shdr.loop_start.saturating_sub(shdr.start)  # relative to sample start
                le = shdr.loop_end.saturating_sub(shdr.start)
                zone = zone.with_loop(loop_mode, ls, le)
            inst_obj.add_zone(zone)

    if inst_obj.zone_count() > 0:
        presets.push(Sf2Preset{ name: phdr.name, bank: phdr.bank, preset_number: phdr.preset })
        instruments.push(inst_obj)
```
Key semantic points to preserve in Cyrius:
- **Index bounds use `saturating_sub(1)`** on `.len()` — e.g. `insts.len()-1`; if a vector is empty
  or length 1 this yields 0 and the `>=` check skips everything (no underflow).
- **Loop points are re-based** to sample start (`loop_start - start`), floored at 0.
- **Range clamping**: preset key/vel range intersects (clamps) the instrument's zone range.
- Each qualifying zone creates a **new** sample+bank entry (samples are duplicated per zone; IDs
  are sequential `SampleId(0), (1), ...`).

---

## 9. PCM sample extraction

```rust
fn pcm16_to_f32(data: &[u8], start_sample: usize, end_sample: usize) -> Vec<f32> {
    let byte_start = start_sample * 2;
    let byte_end   = end_sample * 2;
    if byte_end > data.len() || byte_start > byte_end { return Vec::new(); }  // silent empty
    let num = (byte_end - byte_start) / 2;
    for i in 0..num:
        s = i16::from_le_bytes([slice[i*2], slice[i*2+1]]);
        out.push(s as f32 / 32768.0);      // DIVISOR IS 32768.0, not 32767
}
```
- Sample indices in shdr are **frame indices**, so byte offset = `index * 2` (16-bit mono PCM).
- Out-of-range or inverted range → **empty vec, no error**.
- Normalization divisor is **32768.0** (so full-scale negative -32768 maps to -1.0 exactly;
  positive +32767 maps to ~0.99997). Cyrius: `f64_div(i16_to_f64(s), 32768.0)` then narrow to f32
  bit-pattern if the sample store is f32.
- Cyrius float note: samples are f32 in Rust. In an f64 world you may store as f64 bit-patterns;
  keep the `/32768.0` divisor and the i16 sign handling (`s>=0x8000 → s-0x10000`).

Stdlib helper needed: read a run of LE i16 from a byte buffer and convert to normalized floats.

---

## 10. Downstream constructor signatures the port must call

```rust
Instrument::new(name)                 // -> Instrument
Instrument::add_zone(&mut self, Zone)
Instrument::zone_count() -> usize
Instrument::zones() -> &[Zone]

SampleBank::new() -> SampleBank
SampleBank::add(&mut self, Sample) -> SampleId   // SampleId(u32) = current len, then push
SampleBank::get(id) -> Option<&Sample>
SampleBank::len() -> usize

Sample::from_mono(Vec<f32>, sample_rate: u32) -> Sample
Sample::with_name(self, name) -> Sample
Sample::frames() -> usize

Zone::new(SampleId) -> Zone           // defaults: key 0..127, vel 1..127, root 60
Zone::with_key_range(lo:u8, hi:u8)
Zone::with_vel_range(lo:u8, hi:u8)
Zone::with_root_note(note:u8)
Zone::with_loop(LoopMode, start:usize, end:usize)
```
`SampleId` is a newtype `struct SampleId(pub u32)` → plain i64 in Cyrius.
`LoopMode` enum variants used here: `OneShot`, `Forward`, `LoopSustain` (also has `PingPong`,
`Reverse` unused by this parser) → integer tags in Cyrius.
`Zone` public range fields (used in tests): `key_lo, key_hi, vel_lo, vel_hi, root_note,
loop_mode, loop_start, loop_end` (loop_start/end are `usize`).

---

## 11. Inline tests (`#[cfg(all(test, feature="std"))]`) — port as fixtures

Two test modules: `test_helpers` (SF2 byte builders) and `tests`.

### `test_helpers` builders (needed to construct test SF2 blobs)
- `write_u16_le / write_u32_le / write_i16_le / write_fourcc` — LE appenders.
- `write_fixed_string(buf, s, len)` — copy up to `len` bytes, NUL-pad to `len`.
- `make_chunk(id, data)` — id + LE u32 len + data + **1 pad byte if len is odd**.
- `make_list(form_type, sub_chunks)` — `"LIST"` + LE u32 innerlen + form_type + concatenated subs.
- `make_sf2(info, sdta, pdta)` — `"RIFF"` + LE u32 innerlen + `"sfbk"` + info + sdta + pdta.
- `make_phdr(name,preset,bank,bag_ndx)` — 20-byte name + preset + bank + bag_ndx + 3×u32 zero
  (library/genre/morphology) = 38 bytes.
- `make_bag(gen_ndx, mod_ndx)` — 2 u16 = 4 bytes.
- `make_gen(oper, amount)` — u16 oper + **i16** amount = 4 bytes.
- `make_gen_range(oper, lo, hi)` — `amount = (lo as i16) | ((hi as i16) << 8)`.
- `make_inst(name, bag_ndx)` — 20-byte name + u16 = 22 bytes.
- `make_shdr(name,start,end,loopS,loopE,rate,pitch,type)` — 20-name +5×u32 +pitch(u8)
  +pitchcorr(u8 0) +samplelink(u16 0) +type(u16) = 46 bytes.
- `make_pcm16(samples:&[f32])` — `(s*32767.0).round().clamp(-32768,32767) as i16` LE. **Encoder
  uses 32767 scale; decoder uses 32768 divisor — asymmetric on purpose; keep both.**
- `build_minimal_sf2(preset_name, sample_data, root_key, key_lo,key_hi, vel_lo,vel_hi,
  loop_mode, loop_start, loop_end)` — assembles a full valid SF2 with: INFO/ifil(2,1), sdta/smpl,
  and pdta with phdr(2 recs incl "EOP"), pbag(2), pgen(gen41=inst0, gen0), inst(2 incl "EOI"),
  ibag(2: gen0 and gen4), imod(empty), igen(gen43 keyrange, gen44 velrange, gen54 loopmode,
  gen53 sampleid=0, gen0 term), shdr(2: "Sample" and "EOS"), plus empty pmod/imod chunks.
  The igen has **4 real gens + terminal**, so ibag second record points at gen index 4.

### `tests` cases (assertions to replicate)
| Test | Setup | Expect |
|---|---|---|
| `reject_too_small` | `parse(&[0;4])` | Err |
| `reject_non_riff` | 12 bytes, id="NOTF" | Err |
| `reject_wrong_form_type` | RIFF + size + "WAVE" | Err |
| `parse_minimal_sf2` | Piano, 100 samples, root 60, key 36..84, vel 0..127 | 1 preset name "Piano"; 1 instrument; 1 zone; zone.key_lo=36, key_hi=84, root_note=60, loop_mode=OneShot; bank.len=1; sample frames=100 |
| `pcm16_conversion_accuracy` | [0,0.5,-0.5,1,-1] round-trip | each within 0.001 |
| `forward_loop_preserved` | 200 samples, loop_mode=1, loopS=50, loopE=150 | loop_mode=Forward, loop_start=50, loop_end=150 |
| `loop_sustain_mode_3` | loop_mode=3 | loop_mode=LoopSustain |
| `velocity_range_preserved` | vel 32..96 | vel_lo=32, vel_hi=96 |
| `pcm16_empty` | `pcm16_to_f32(&[],0,0)` | empty |
| `pcm16_out_of_bounds` | `pcm16_to_f32(&[0,0],0,100)` | empty |
| `reject_chunk_size_overflow` | RIFF + "sfbk" + LIST chunk with size=u32::MAX | Err (no panic) |

Note: in `forward_loop_preserved` loop points are 50/150 because shdr.start=0, so
`loop_start - start = 50` unchanged. If a real file has nonzero start, subtract it.

---

## 12. Cyrius porting checklist / gotchas

- Everything LE; no big-endian path exists.
- Record SIZE constants are exact: phdr **38**, bag **4**, gen **4**, inst **22**, shdr **46**.
- `count = chunklen / SIZE` (floor); ignore trailing partial bytes.
- Range generators: lo=low byte, hi=high byte of a 16-bit amount.
- gen `amount` is signed i16 for non-range gens (e.g. sample_modes cast via u16), and used as
  `usize` index for inst(41)/sample(53) — a negative amount would produce a huge index and be
  skipped by the `>= len-1` guard; replicate by treating amount as its raw u16 for indexing.
- All `.len()-1` uses are **saturating** (never underflow on empty).
- Loop points re-based to sample start, floored at 0 (`saturating_sub`).
- Preset range clamps (intersects) instrument range.
- ROM samples (`sample_type & 0x8000`) skipped.
- PCM decode divisor **32768.0**; encoder (test) scale **32767.0** — keep asymmetry.
- Samples duplicated per zone; SampleIds sequential from 0.
- Zero-zone instruments and their presets are dropped, keeping the two vectors parallel.
- No serde in Cyrius (drop derives). `Result<T,NidhiError>` → negative i64 codes; success →
  the 3-list result struct.
```
```
