# nidhi — Current State

> Refreshed every release. CLAUDE.md is preferences/process/procedures
> (durable); this file is **state** (volatile).

## Version

**2.3.0** — 2026-11-18. The rest of 2.2.2's hardening:
- every constructor nidhi owns, and the SFZ and SF2 importers, report a refused allocation instead
  of writing through it;
- SF2 records are read lazily (a 42 MB record table costs 2,664 B, not 251.7 MB);
- SFZ has a `(ptr, len)` entry point (`n_sfz_parse_n`);
- two guard-page fuzz harnesses.

No output changes on valid input.
Follows 2.2.2 (newest dependencies and a P(-1) sweep: hostile-file crashes closed, nine oracle
defects fixed by decision in ADRs 0006 to 0012, render paths 8 to 38 % faster), 2.2.1 (cyrius 6.7.18; high-pass, band-pass and notch voices no longer allocate), 2.2.0 (cyrius 6.7.9; a consumer needs 6.7.5 or later), 2.1.3 (cyrius 6.6.20: WAV
loading, time-stretch and the delay effect repaired, render output moved by at most −243 dB, and
a sample rate naad refuses no longer crashes the engine),
2.1.2 (cyrius 6.6.3), 2.1.1 (`note_on` allocates nothing, and a reused voice no longer
inherits the previous note's cutoff), 2.1.0 (zone volume applied, streaming reader),
2.0.7 (performance), 2.0.6 (latent hazards), 2.0.5 (coverage backfill), 2.0.4 (oracle retired),
2.0.3 (crossfade seam), 2.0.2 (P-1 sweep), 2.0.1 (toolchain catch-up), 2.0.0 (the port).

## Toolchain

- **Cyrius pin**: `6.7.18` (in `cyrius.cyml [package].cyrius` — the single source of truth; CI
  reads it rather than hardcoding a version)

## Source

- Rust reference: **retired in 2.0.4.** The 7180-line crate lives in git history
  (`git show <rev>:rust-old/src/<mod>.rs`); its build identity and full dependency lockfile are
  recorded at [`docs/port/oracle-build-identity.md`](../port/oracle-build-identity.md), and the
  behaviour it pinned is asserted by `tests/golden.tcyr`.
- Cyrius port: **all 14 modules ported**, `src/*.cyr`, listed in dependency order under
  `cyrius.cyml [lib].modules`:
  `error` → `f64_util` → `loop_mode` → `envelope` → `zone` → `sample` → `instrument` →
  `capture` → `stretch` → `effect_chain` → `io` → `sf2` → `sfz` → `engine`.
- `[build].entry` is `programs/smoke.cyr` — a build-chain smoke test, not the product. nidhi is a
  library; consumers take `dist/nidhi.cyr` (built by `cyrius distlib`).

## Tests

- **16 suites / 1,059 assertions / 0 failures** (`cyrius test`). The build prints no `src/`
  warning, and `cyrius fmt --check` is clean on every source file.
- **Refused allocations**: `tests/alloc_refused.tcyr` replaces the stdlib `alloc` and refuses each
  allocation of every nidhi constructor in turn. `tests/sf2.tcyr` and `tests/sfz.tcyr` lower
  `ALLOC_MAX` to refuse inside the importers.
- Render path **and `note_on`** asserted allocation-free: `alloc_used()` delta of 0 across 20
  blocks at 8 and 64 voices in every filter mode, and 0 bytes per note_on/note_off pair
  (`tests/engine.tcyr`)
- **Fuzz**: 4 harnesses, 4 passed, 0 crashes.
  - `fuzz_sf2` and `fuzz_sfz` take small random and mutated inputs. `fuzz_sfz` also bounds parse
    heap per input byte, and checks that both SFZ entry points agree.
  - `guard_sf2` and `guard_sfz` (2.3.0) put each input against a `PROT_NONE` page, so a read past
    its end faults. They also feed structure-aware inputs up to 62 KB (SF2) and 200 KB tokens (SFZ).
    Each falls back to a plain buffer, and says so, where `cyr_mmap` is unavailable (agnos).
  - The 2.2.2 SF2 and SFZ bombs are also pinned as regression tests in `tests/sf2.tcyr` and
    `tests/sfz.tcyr`.
- **Benchmarks**: `tests/nidhi.bcyr`, 16 measurements reproducing the 7 Rust criterion
  benchmarks. See [`BENCHMARKS.md`](../../BENCHMARKS.md) and
  [`bench-history.csv`](../../bench-history.csv).

## Dependencies

Direct (declared in `cyrius.cyml`):

- **stdlib** — string, fmt, alloc, vec, str, syscalls, io, args, assert, bench, tagged, result,
  fnptr, math, ganita, hashmap, thread (no `bayan` since 2.2.1: shravan declares its own)
- **naad** 2.3.5 — DSP (SVF filters, ADSR, LFOs, effects, voice management, interpolation)
- **sankoch** 2.8.9 — compression, shravan's dep. Kept above shravan: both define `detect_format`,
  and the last definition wins every call. Since shravan 2.9.2 only a direct caller sees it
- **shravan** 2.12.4 — audio codecs, behind `src/io.cyr`. `n_load_wav` reads the file itself and
  decodes through shravan's `codec_open` dispatch (WAV, AIFF, FLAC, OGG, MP3, ALAC, AAC/MP4,
  Opus). `n_load_wav_from_memory` and the stream reader are WAV-only, as in the oracle
- **hisab** 3.5.0 — math

Transitive, resolved into `lib/`: **goonj** 2.3.2, **bayan** 1.11.1 (`dist/bayan-json.cyr`,
shravan's). **sakshi** 2.5.10 is a stdlib leaf from the 6.7.18 snapshot since hisab 3.4.2.

`dist/nidhi.deps` names the one stdlib leaf the bundle needs beyond its deps: **hashmap** (the SFZ
parser, since 2.2.2). A consumer that vendors `dist/nidhi.cyr` by hand needs it in its own
`[deps].stdlib`.

## Consumers

**dhvani** (audio engine), and thereby **shruti** (DAW) — nidhi replaces
`shruti-instruments::sampler`. Both vendor `dist/nidhi.cyr`.

## Known hazards

Everything is pinned in [`roadmap.md`](roadmap.md); this is the short list of what a consumer
could actually hit.

- **A consumer needs cyrius 6.7.5 or later.** sankoch uses `loop { }`, so anything older fails at
  compile time. Re-measured for 2.3.0 on every pin from 6.7.5 to 6.7.18: the SFZ parser avoids
  6.7.9's `f64_parse_n` on purpose, or 6.7.5 to 6.7.8 would fail.
- **`n_sfz_to_zones` and `n_sfz_to_instrument` can return a negative code** since 2.3.0:
  `NIDHI_ERR_INVALID_PARAMETER` for a 0 or error-code file, `NIDHI_ERR_IMPORT` on a refused
  allocation. A consumer that took `vec_len` of the result unchecked must check it first.
- **A refused allocation inside a naad constructor still stops at the null page**, by naad's
  design (its ADR-0006). nidhi checks only what it allocates itself, so `n_engine_new` can still
  fault on a heap that runs out while it builds naad's envelopes, LFOs and smoothers. dhvani pins 6.6.10, so it
  moves its toolchain to take 2.2.0 or later.
- **A consumer on shravan older than 2.9.2 must keep sankoch above shravan** in its own manifest
  once it is on cyrius 6.6.20 or later, or `n_load_wav` refuses every file. dhvani already does.
  From shravan 2.9.2 the order decides only which `detect_format` a direct caller gets.
- **Per-zone bus routing does not work.** `output=` is parsed from SFZ, inherited and stored,
  and **read by nothing** — the same shape `volume_db` had before 2.1.0. Planned for 2.5.0. The
  README says so too rather than listing it as a feature.
- **A reverse-loop zone is silent unless it has a start offset.** `LOOP_REVERSE` starts at
  `sample_offset` (0 by default), steps backwards and ends after one frame. **Inherited from the
  oracle**; SFZ never sets this mode, only the API does. Deferred by the maintainer in 2.2.2;
  planned for 2.6.0.
- **One zone per note, and SFZ `group=` means round-robin.** `note_on` round-robins since 2.2.2
  (ADR 0012), but it still starts one voice per note, where SFZ sounds every matching region. It
  also maps `group=` to round-robin, as the oracle did, where SFZ means an `off_by` exclusive class.
  So co-matching zones that share a `group=` alternate. Planned for 2.4.0.
- **SF2 spec gaps, inherited from the oracle:** sampleModes 2 loops (the spec says it should not),
  `original_pitch` 128-255 is used as the root (the spec says 60), and `loop_end` past the sample
  is kept. Planned for 2.6.0.
- **A file with more than two channels keeps only its front pair** (ADR 0010). A 4-channel
  L-C-R-S AIFF therefore loads as left and centre.
- **WSOLA amplifies output up to ~588x** where `window_sum` falls under the `1e-6` normalise
  threshold. **Inherited from the oracle**, identical threshold and guard — a port-faithful
  defect, not a port defect. Fixing it is a deliberate divergence needing its own ADR.
- **About 80 top-level names skip the `n_`/`N` prefix** (86 until 2.2.2 removed six dead SFZ
  helpers; 2.3.0 swapped five `Sf2*` structs and eight helpers for twelve `sf2_*` field readers).
  0 collisions measured (all 306 of nidhi's top-level names against the 9,579 in `lib/`, 2.3.0),
  so this is forward risk only —
  [ADR 0005](../adr/0005-namespace-prefix-scope.md), and
  [architecture 002](../architecture/002-one-flat-namespace.md) for why it matters at all.

Deliberate divergences from the Rust oracle, each with an ADR: the loop-crossfade seam
([0001](../adr/0001-loop-crossfade-seam.md)), NaN clamping to a bound
([0002](../adr/0002-nan-clamps-to-the-bound.md)), zone `volume` being applied at all
([0004](../adr/0004-apply-zone-volume-db.md)), and, from 2.2.2:
- voice allocation following nidhi's own voice state ([0006](../adr/0006-nidhi-voice-state-decides-allocation.md));
- filter velocity tracking lasting the note ([0007](../adr/0007-filter-velocity-tracking-lasts-the-note.md));
- no crossfade on a released sustain loop ([0008](../adr/0008-released-sustain-loop-skips-the-crossfade.md));
- playback speed including the sample rate ([0009](../adr/0009-playback-speed-includes-sample-rate.md));
- the multichannel front pair ([0010](../adr/0010-multichannel-loads-front-pair.md));
- the reverb and chorus parameters ([0011](../adr/0011-effect-slots-get-the-parameters-meant.md));
- note_on round-robin and real SFZ sequences ([0012](../adr/0012-round-robin-plays.md)).

## rust-old/ — retired (2.0.4)

Removed from the working tree; **377 MB reclaimed** (repo 525 MB → 148 MB). Nothing was lost:

- The 1002 tracked files are in git history — `git show HEAD~1:rust-old/src/engine.rs` works.
- The two files that were **not** tracked, and would have been lost, are preserved:
  `Cargo.lock` (all 88 dependency versions) and the rustc identity, both in
  [`docs/port/oracle-build-identity.md`](../port/oracle-build-identity.md);
  `bench-history-rust.csv` is now tracked (the blanket `*.csv` ignore was excluding it *and* the
  live benchmark series).
- The behaviour it pinned is asserted by `tests/golden.tcyr` — 43 assertions against values
  captured from a live oracle build, closing the gap the 2.0.3 audit named as the single largest
  untested surface in the port (stretch output *values*, previously untested on **both** sides).

## Next

See [`roadmap.md`](roadmap.md).
