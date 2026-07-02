# Port Spec: `src/sfz.rs` — SFZ Text Parser (v1 + v2 subset)

Source: `/home/macro/Repos/nidhi/src/sfz.rs` (1438 lines). Target: Cyrius (everything-is-i64,
heap structs with untyped fields, `#derive(accessors)` for `Type_field`/`Type_set_field`,
f64 bit-patterns via `f64_add`/`f64_mul`/…, negative-int error codes, no serde/generics/traits).

Converts SFZ text into nidhi `Instrument` + `Zone` structs. Line-based, whitespace-tokenized,
`key=value` opcodes. Unknown opcodes silently ignored; malformed tokens (no `=`, empty key/value)
skipped. `parse` is infallible in practice (always returns `Ok`).

---

## 1. Public API (Rust signatures)

```rust
pub fn parse_note_or_number(s: &str) -> Option<u8>          // note name or numeric -> MIDI 0..127
pub fn parse(input: &str) -> Result<SfzFile>               // Result = core::Result<T, NidhiError>
pub struct SfzRegion { /* 40 fields, all pub except none; see §3 */ }
impl SfzRegion { pub fn new() -> Self; fn apply_opcode(&mut self,k:&str,v:&str); fn inherit_from(&mut self,parent:&SfzRegion); }
pub struct SfzFile { pub global, pub groups:Vec<SfzRegion>, pub regions:Vec<SfzRegion>,
                     group_indices:Vec<Option<usize>> /*PRIVATE*/, pub default_path:Option<String>, pub includes:Vec<String> }
impl SfzFile {
  pub fn to_instrument(&self, name:&str, sample_rate:f32) -> (Instrument, Vec<String>)
  pub fn to_zones(&self, sample_rate:f32) -> Vec<(Zone, String)>
}
// private free fns:
fn map_loop_mode(Option<&str>) -> LoopMode
fn map_fil_type(Option<&str>) -> FilterMode
fn map_fil_veltrack(f32) -> f32
fn parse_header(&str) -> Option<HeaderKind>
fn split_opcode(&str) -> Option<(&str,&str)>
enum HeaderKind { None, Control, Global, Group, Region, Curve }
```

Cyrius shapes: `SfzRegion`/`SfzFile` become heap structs with `#derive(accessors)`. `Option<String>`
→ pointer-or-null (0 = None). `Option<u8>`/`Option<usize>` for `key`/return values → use a sentinel:
recommend returning `-1` for None and `0..127` for Some from `parse_note_or_number`. `Vec<T>` →
dynamic array struct (ptr,len,cap). `(String,u8,f32)` cc tuple → 3-field heap struct. Floats are f64
bit-patterns; the Rust source is f32 but Cyrius has only f64 — store as f64, all arithmetic via
`f64_*`. Error codes: `parse` never errors here; return an `Ok` sentinel (e.g. pointer to SfzFile,
or 0-on-error convention — but no error path exists in the current code).

---

## 2. `parse_note_or_number(s) -> Option<u8>` (lines 22-78)

Algorithm, IN ORDER:
1. Try `s.parse::<u8>()` (decimal 0..=255 that fits u8). If ok, return it directly (NOTE: a numeric
   like "200" fails u8 parse and falls through; "60" returns 60). **In Cyrius: parse decimal int; if
   it succeeds AND fits 0..=255, return it.**
2. If empty bytes → None.
3. First byte, lowercased, maps to semitone base: `c=0 d=2 e=4 f=5 g=7 a=9 b=11`. Any other → None.
4. `idx=1`, `accidental=0`. Look at `bytes[idx]`:
   - `#` or `s` → accidental = +1, idx += 1.
   - `b` **only if** `idx+1 < len` AND `bytes[idx+1]` is ASCII digit → accidental = -1, idx += 1.
     (This disambiguates note "B" from flat: `bb3` = B-flat but plain `b` w/o following digit stays note B.)
   - else leave idx at 1.
5. Octave string = `s[idx..]`; parse as `i32` (may be negative, e.g. `c-1`). Parse failure → None.
6. `midi = (octave + 1) * 12 + note_base + accidental`.
7. If `0 <= midi <= 127` return `midi as u8`, else None.

Test vectors (line 1264): `60→60`, `c4→60`, `C4→60`, `f#3→54`, `eb4→63`, `b4→71`, `c-1→0`, `g9→127`,
`""→None`, `xyz→None`.

Cyrius needs: byte-at-index, ascii-lowercase (`b|32` if in 'A'..'Z'), is-ascii-digit
(`c>='0' && c<='9'`), substring slice, decimal-int-parse (signed, handles leading `-`).

---

## 3. `SfzRegion` — 40 fields with defaults (lines 85-207)

`Default::default()` values are LOAD-BEARING because `inherit_from` uses "== default" to decide
whether to inherit. Store exactly these defaults:

| field | type | default | notes |
|---|---|---|---|
| sample | Option<String> | None | null ptr |
| lokey | u8 | 0 | |
| hikey | u8 | 127 | |
| lovel | u8 | 1 | |
| hivel | u8 | 127 | |
| pitch_keycenter | u8 | 60 | |
| tune | i32 | 0 | cents |
| volume | f32 | 0.0 | dB |
| pan | f32 | 0.0 | -100..100 |
| loop_mode | Option<String> | None | |
| loop_start | usize | 0 | |
| loop_end | usize | 0 | |
| group | u32 | 0 | round-robin/seq_position |
| ampeg_attack | f32 | 0.0 | sec |
| ampeg_decay | f32 | 0.0 | sec |
| ampeg_sustain | f32 | 100.0 | 0..100 |
| ampeg_release | f32 | 0.0 | sec |
| cutoff | f32 | 0.0 | Hz |
| fil_veltrack | f32 | 0.0 | cents |
| fileg_attack | f32 | 0.0 | sec |
| fileg_decay | f32 | 0.0 | sec |
| fileg_sustain | f32 | 100.0 | 0..100 |
| fileg_release | f32 | 0.0 | sec |
| fileg_depth | f32 | 0.0 | cents |
| transpose | i32 | 0 | semitones |
| offset | usize | 0 | frames |
| end | usize | 0 | frames (0 = full) |
| resonance | f32 | 0.0 | Q |
| fil_type | Option<String> | None | |
| key | Option<u8> | None | shorthand |
| pitchlfo_freq | f32 | 0.0 | Hz |
| pitchlfo_depth | f32 | 0.0 | cents |
| fillfo_freq | f32 | 0.0 | Hz |
| fillfo_depth | f32 | 0.0 | cents |
| fil_keytrack | f32 | 0.0 | cents 0..1200 |
| output | u8 | 0 | bus idx |
| cc_modulations | Vec<(String,u8,f32)> | empty | |

---

## 4. `apply_opcode(key, value)` — opcode table (lines 216-405)

Match `key` exactly (case-sensitive). Aliases share an arm. On numeric parse failure the field is
left unchanged (opcode silently no-ops). Clamps are load-bearing — reproduce exactly.

| opcode key(s) | parse as | transform / clamp | field set |
|---|---|---|---|
| `sample` | string (raw) | — | sample = Some(v) |
| `lokey` | parse_note_or_number | Some→set | lokey |
| `hikey` | parse_note_or_number | Some→set | hikey |
| `key` | parse_note_or_number | Some→set | key |
| `lovel` | u8 | — | lovel |
| `hivel` | u8 | — | hivel |
| `pitch_keycenter` | parse_note_or_number | — | pitch_keycenter |
| `tune` | i32 | — | tune |
| `volume` | f32 | — | volume |
| `pan` | f32 | clamp(-100,100) | pan |
| `loop_mode`, `loopmode` | string (raw) | — | loop_mode = Some(v) |
| `loop_start`, `loopstart` | usize | — | loop_start |
| `loop_end`, `loopend` | usize | — | loop_end |
| `seq_position` | u32 | — | group |
| `group` | u32 | — | group |
| `ampeg_attack` | f32 | max(0.0) | ampeg_attack |
| `ampeg_decay` | f32 | max(0.0) | ampeg_decay |
| `ampeg_sustain` | f32 | clamp(0,100) | ampeg_sustain |
| `ampeg_release` | f32 | max(0.0) | ampeg_release |
| `cutoff` | f32 | max(0.0) | cutoff |
| `fil_veltrack` | f32 | — | fil_veltrack |
| `fileg_attack` | f32 | max(0.0) | fileg_attack |
| `fileg_decay` | f32 | max(0.0) | fileg_decay |
| `fileg_sustain` | f32 | clamp(0,100) | fileg_sustain |
| `fileg_release` | f32 | max(0.0) | fileg_release |
| `fileg_depth` | f32 | clamp(-9600,9600) | fileg_depth |
| `transpose` | i32 | — | transpose |
| `offset` | usize | — | offset |
| `end` | usize | — | end |
| `resonance`, `fil_resonance` | f32 | max(0.0) | resonance |
| `fil_type`, `filtype` | string (raw) | — | fil_type = Some(v) |
| `pitchlfo_freq` | f32 | max(0.0) | pitchlfo_freq |
| `pitchlfo_depth` | f32 | — | pitchlfo_depth |
| `fillfo_freq` | f32 | max(0.0) | fillfo_freq |
| `fillfo_depth` | f32 | — | fillfo_depth |
| `fil_keytrack` | f32 | clamp(0,1200) | fil_keytrack |
| `output` | u8 | — | output |
| `*_oncc<N>` (contains `_oncc`) | see below | | cc_modulations.push |
| anything else | — | — | ignored |

**CC modulation fallback arm (lines 393-402):** if `key.contains("_oncc")`: find first index of
substring `"_oncc"`; `param = key[..pos]`; `cc_str = key[pos+5..]`; parse `cc_str` as u8 and `value`
as f32; if BOTH ok, push `(param, cc, depth)`. Example: `cutoff_oncc74=2400` → `("cutoff", 74, 2400.0)`.
Cyrius: needs substring-find (returns index or -1) and slice.

Cyrius stdlib needs for §4: exact string compare (`streq`), substring-contains, substring-index-of,
decimal int parse (signed & unsigned), float parse (f64), `f64_max`, `f64_clamp` (or min+max).

---

## 5. `inherit_from(parent)` — inheritance (lines 412-526)

Implements global→group→region. For EACH field, copy parent's value into self ONLY IF self is still
at its default AND parent differs from default. Pattern per field:

```
if self.FIELD == DEFAULT && parent.FIELD != DEFAULT { self.FIELD = parent.FIELD; }
```

Option/String fields use `is_none()` instead of `== default`:
- `sample`: if self.sample is None → clone parent.sample (unconditional on parent).
- `loop_mode`: if None → clone parent.loop_mode.
- `fil_type`: if None → clone parent.fil_type.
- `key`: if None → copy parent.key.
- `cc_modulations`: if self empty AND parent non-empty → clone parent's vec.

Numeric fields and their default sentinels (all the "== DEFAULT" checks): lokey(0), hikey(127),
lovel(1), hivel(127), pitch_keycenter(60), tune(0), volume(0.0), pan(0.0), loop_start(0), loop_end(0),
group(0), ampeg_attack(0.0), ampeg_decay(0.0), ampeg_sustain(100.0), ampeg_release(0.0), cutoff(0.0),
fil_veltrack(0.0), fileg_attack(0.0), fileg_decay(0.0), fileg_sustain(100.0), fileg_release(0.0),
fileg_depth(0.0), transpose(0), offset(0), end(0), resonance(0.0), pitchlfo_freq(0.0),
pitchlfo_depth(0.0), fillfo_freq(0.0), fillfo_depth(0.0), fil_keytrack(0.0), output(0).

NOTE the documented caveat (lines 416-417): explicit zero in a child is indistinguishable from
unset, so an explicit `volume=0` child will still inherit a nonzero parent volume. Preserve this
exact behavior — do NOT add "was-explicitly-set" tracking. Float equality is exact bit compare
(f32 `==`); in Cyrius compare f64 bit patterns for equality against the default constant.

---

## 6. `parse(input)` — main loop (lines 795-895)

State: `global:SfzRegion`, `groups:Vec`, `regions:Vec`, `group_indices:Vec<Option<usize>>`,
`current_header:HeaderKind = None`, `current_group_idx:Option<usize> = None`,
`default_path:Option<String> = None`, `includes:Vec<String>`.

For each line (split on `\n`; `.lines()` also strips a trailing `\r`):
1. `line = line.trim()` (both ends, ascii whitespace).
2. If empty OR starts_with `"//"` → skip (comment).
3. If starts_with `"#include"`: `path = line.strip_prefix("#include").trim().trim_matches('"')`
   (strips ONE-char-class `"` from both ends). If non-empty → `includes.push(path)`. Continue.
4. Tokenize: `line.split_whitespace()` → Vec<&str> (splits on runs of ascii whitespace, no empties).
5. For each token:
   a. `parse_header(token)`: if token is `<name>` (starts `<`, ends `>`), inner name matched:
      `control→Control, global→Global, group→Group, region→Region, curve→Curve, else→None(no match)`.
      On a matched header set `current_header`; additionally:
      - Group: `groups.push(new()); current_group_idx = Some(groups.len()-1)`.
      - Region: `regions.push(new()); group_indices.push(current_group_idx)`.
      - Control/Global/Curve: just set the header.
      Then `continue` to next token.
   b. Else `split_opcode(token)` → `(key,value)` split at FIRST `=`; if key or value empty → skip.
      Dispatch by `current_header`:
      - Control: if `key=="default_path"` → `default_path = Some(value)`; else ignore.
      - Global: `global.apply_opcode`.
      - Group: `groups.last_mut().apply_opcode`.
      - Region: `regions.last_mut().apply_opcode`.
      - Curve: ignored (stored for future — currently dropped).
      - None (opcodes before any header): treated as **global** → `global.apply_opcode`.

Return `SfzFile { global, groups, regions, group_indices, default_path, includes }` wrapped `Ok`.

`split_opcode` (916): find first `=`; key=before, value=after; if key.is_empty()||value.is_empty()→None.
Note only the FIRST `=` splits, so `a=b=c` → key="a", value="b=c".

Cyrius stdlib needs for §6: line iteration (split on `\n`, strip trailing `\r`), trim (ascii-ws both
ends), starts_with, strip_prefix, trim_matches(char), split_whitespace (into token array),
find(char)→index.

---

## 7. `to_zones(sample_rate) -> Vec<(Zone,String)>` (lines 599-747)

For each region i (0..regions.len()):
1. `merged = region.clone()`.
2. If `group_indices[i]` is Some(gi) and `groups[gi]` exists → `merged.inherit_from(&groups[gi])`.
3. `merged.inherit_from(&global)` (always).
4. **`key` shorthand** (617-625): if `merged.key = Some(k)`:
   - if `lokey==0 && hikey==127` → set lokey=k, hikey=k.
   - if `pitch_keycenter==60` → set pitch_keycenter=k.
5. If `merged.sample` is None → **skip this region entirely** (no zone emitted).
6. `filename = merged.sample.clone()`.
7. **default_path** (634-641): if `default_path` set AND `!filename.starts_with(prefix)` → prepend:
   `filename = prefix + filename`.
8. `tune_cents = merged.tune + merged.transpose * 100.0` (f32; transpose semitones × 100 cents).
9. `filter_type = map_fil_type(merged.fil_type)` (§8).
10. Build Zone via builder chain (SampleId = `i as u32`, a placeholder; to_instrument remaps later):
```
Zone::new(SampleId(i as u32))
  .with_key_range(lokey, hikey)
  .with_vel_range(lovel, hivel)
  .with_root_note(pitch_keycenter)
  .with_tune(tune_cents)
  .with_volume(volume)                       // dB
  .with_pan(pan / 100.0)                      // -100..100 -> -1..1
  .with_loop(map_loop_mode(loop_mode), loop_start, loop_end)
  .with_filter(cutoff, map_fil_veltrack(fil_veltrack))
  .with_filter_type(filter_type)
  .with_group(group)
```
11. Conditional builders (each returns a new Zone; chain via reassignment):
    - `if resonance > 0.0` → `.with_filter_resonance(resonance)`
    - `if offset > 0` → `.with_sample_offset(offset)`
    - `if end > 0` → `.with_sample_end(end)`
    - **ampeg** (677-693): `has_ampeg = attack!=0 || decay!=0 || sustain!=100 || release!=0`. If so:
      `adsr = AdsrConfig::from_seconds(ampeg_attack, ampeg_decay, ampeg_sustain/100.0, ampeg_release,
      sample_rate)`; `.with_adsr(adsr)`.
    - **fileg** (696-713): `has_fileg = depth!=0 || attack!=0 || decay!=0 || sustain!=100 || release!=0`.
      If so: `fileg = AdsrConfig::from_seconds(fileg_attack, fileg_decay, fileg_sustain/100.0,
      fileg_release, sample_rate)`; `.with_filter_envelope(fileg, fileg_depth)`.
    - **pitch LFO** (716): `if pitchlfo_freq > 0.0 && pitchlfo_depth != 0.0` →
      `.with_pitch_lfo(pitchlfo_freq, pitchlfo_depth)`.
    - **filter LFO** (723): `if fillfo_freq > 0.0 && fillfo_depth != 0.0` →
      `.with_filter_lfo(fillfo_freq, fillfo_depth)`.
    - **key tracking** (730): `if fil_keytrack > 0.0` → `.with_key_tracking(fil_keytrack / 1200.0)`.
    - **output** (737): `if output > 0` → `.with_output_bus(output)`.
12. push `(zone, filename)`.

`AdsrConfig::from_seconds` (envelope.rs:49): `attack_samples=(attack*sr).max(0.0) as u32`,
`decay_samples=(decay*sr).max(0.0) as u32`, `sustain_level=sustain.clamp(0.0,1.0)`,
`release_samples=(release*sr).max(1.0) as u32` (NOTE release floor is 1, not 0). f32→u32 is a
truncating cast. `AdsrConfig` fields: {attack_samples:u32, decay_samples:u32, sustain_level:f32,
release_samples:u32}.

---

## 8. Mapping helpers (lines 750-783)

```
map_loop_mode(Option<&str>):
  "loop_continuous" -> Forward
  "loop_sustain"    -> LoopSustain
  "one_shot"        -> OneShot
  "no_loop" | None  -> OneShot
  anything else     -> OneShot          // default fallthrough

map_fil_type(Option<&str>):
  "hpf_1p" | "hpf_2p" -> HighPass
  "bpf_2p"            -> BandPass
  "brf_2p"            -> Notch
  else (incl lpf_1p/lpf_2p/None/unknown) -> LowPass

map_fil_veltrack(cents) = (cents / 9600.0).clamp(0.0, 1.0)
```

`LoopMode` variants (all 5, port as i64 enum): OneShot=0, Forward=1, PingPong=2, Reverse=3,
LoopSustain=4. `FilterMode`: LowPass=0, HighPass=1, BandPass=2, Notch=3. (Only OneShot/Forward/
LoopSustain and LowPass/HighPass/BandPass/Notch are produced by this parser; PingPong/Reverse never.)

---

## 9. `to_instrument(name, sample_rate) -> (Instrument, Vec<String>)` (lines 566-589)

1. `zones_and_files = self.to_zones(sample_rate)`.
2. `inst = Instrument::new(name)`; `sample_files: Vec<String> = []`.
3. For each `(zone, filename)`:
   - Dedup: find index of `filename` in `sample_files` (string equality). If found → reuse idx.
     Else push filename, idx = new length-1.
   - `zone.sample_id = SampleId(idx as u32)` (overwrite placeholder).
   - `inst.add_zone(zone)`.
4. Return `(inst, sample_files)`.

Dedup preserves first-seen order; two zones referencing the same filename get the SAME SampleId
(test `to_instrument_deduplicates_samples`, line 1233: 3 zones, 2 files, ids 0,0,1).

`SampleId` is a newtype `struct SampleId(pub u32)` (sample.rs:10) — in Cyrius just an i64.
`Instrument::new(name)`, `add_zone(&mut self, Zone)`, `name()->&str`, `zone_count()->usize`,
`zones()->&[Zone]` (instrument.rs).

---

## 10. Grammar summary (what IS and ISN'T handled)

Handled headers: `<control>`, `<global>`, `<group>`, `<region>`, `<curve>`. Unrecognized `<...>`
headers → `parse_header` returns None so the token is treated as a (usually malformed) opcode and
ignored. There is NO `<effect>` header handling (mentioned in your prompt but absent here) — `<effect>`
would be an unrecognized header and dropped. `<curve>` is recognized but its opcodes are DISCARDED
(only prevents a break; `curve_index`, `vNNN` opcodes ignored).

Directives: `#include "path"` collected into `includes` (NOT resolved/expanded — the parser records
paths only; the caller is responsible for inclusion). `#define` is NOT handled at all (no macro
substitution; a `#define $var x` line would fail the `#include` check, then tokenize and produce
junk opcodes that get ignored). `default_path` handled ONLY inside `<control>`.

Comments: only `//` line-comments (whole line must start with `//` after trim). No `/* */` block
comments, no inline `//` mid-line stripping. Multiple opcodes per line supported (whitespace-sep).

---

## 11. Inline tests (37 total, lines 926-1438) — port as parity checks

Gated `#[cfg(all(test, feature="std"))]`. Key ones to replicate:
`parse_empty_file`, `parse_single_region`, `parse_with_global_defaults`,
`parse_with_group_inheritance`, `round_trip_to_instrument` (pan=50→0.5, tune=5, volume=-3),
`loop_mode_mapping`, `invalid_opcode_ignored`, `comments_and_blank_lines_skipped`,
`region_overrides_group_overrides_global` (region volume=-2 wins; pan inherited 25→0.25),
`loop_mode_parsed_in_region`, `filter_opcodes_parsed` (fil_veltrack 4800/9600=0.5),
`multiple_groups` (2 groups, 3 regions; region→nearest preceding group),
`region_without_sample_skipped`, `adsr_envelope_from_sfz`,
`to_instrument_deduplicates_samples` (ids 0,0,1), `note_name_parsing`, `note_names_in_opcodes`,
`key_shorthand_opcode` (key=60 → lo=hi=root=60), `control_header_default_path`
(`samples/piano/` + `c4.wav` = `samples/piano/c4.wav`), `curve_header_does_not_break_parsing`,
`transpose_adds_to_tune` (tune=10 + transpose=2 → 210 cents), `fil_type_maps_to_filter_mode`,
`offset_and_end_opcodes`, `resonance_opcode`, `fileg_opcodes_wired_to_zone`,
`ampeg_wired_to_zone_adsr`, `loop_sustain_mode_in_sfz`, `pitchlfo_opcodes_wired_to_zone`,
`fillfo_opcodes_wired_to_zone`, `fil_keytrack_opcode` (600/1200=0.5),
`include_directives_collected` (2 includes, quotes stripped),
`cc_modulation_opcodes_parsed` (`volume_oncc1=6` → ("volume",1,6.0)), `output_opcode_wired_to_bus`.

---

## 12. Cyrius stdlib helpers required (checklist)

String: `streq`, `starts_with`, `substring/slice(start,end)`, `find_char(=,<,>)→idx`,
`find_substr("_oncc","//","#include")→idx`, `contains`, `trim` (ascii ws both ends),
`trim_start_matches(prefix)`, `trim_matches(char)`, `split_whitespace→token array`,
`lines→line array` (strip trailing `\r`), `byte_at`, `ascii_lower(byte)`, `is_ascii_digit(byte)`,
string concat/builder (for default_path prepend, capacity = prefix.len+filename.len).
Numeric: signed int parse (`i32`/`i64`, handles leading `-`), unsigned parse (`u8`/`u32`/`usize`
with range check — u8 parse must REJECT >255 so `parse_note_or_number` falls through to note logic),
f64 parse. Float ops (f64 bit-pattern): `f64_add`, `f64_mul`, `f64_div`, `f64_max`, `f64_min`
(for clamp), `f64_eq`/`f64_ne` (exact bit compare for the inherit defaults), f64→i64 truncating
cast (for `as u32` in from_seconds), f64 literals for constants (100.0, 9600.0, 1200.0, -9600.0).
Collections: growable array (push, last_mut, get(i), len, is_empty, position/find, clone).
Option/null: pointer-null convention for `Option<String>`; `-1` sentinel for `Option<u8>` returns.
