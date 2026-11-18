# nidhi — Roadmap

> **Forward-facing only.** What ships next, in what order, and why anything not next is not next.
> Shipped work lives in [`CHANGELOG.md`](../../CHANGELOG.md); current state is in
> [`state.md`](state.md); decisions are in [`docs/adr/`](../adr/).

Current: **2.2.2**. Each minor release below has one theme, and each item says why it is there.

- **The order is by readiness, then harm.** 2.3.0 needs no decision and changes nothing on valid
  input. 2.4.0 fixes the largest audible SFZ gap, but needs a design ADR first. 2.5.0 adds
  capability no consumer is blocked on yet. 2.6.0 holds the oracle behaviours the maintainer
  deferred in 2.2.2.
- **Patch releases (2.x.y)** carry dependency and toolchain bumps and repairs on whichever line is
  current, as 2.2.1 and 2.2.2 did. A patch release does not move the consumer's cyrius floor.
- **An oracle divergence needs an ADR.** Any item that changes what the Rust oracle did gets one,
  plus a CHANGELOG section that says what moved.

---

## 2.3.0 — finish the 2.2.2 hardening

Leftovers from the 2.2.2 P(-1) audit. No sound changes on valid input. It is a minor release
because one item adds a public entry point and one adds a `#must_use`.

- [ ] **Unchecked `alloc` in the remaining constructors**: `n_sample_from_mono`,
      `n_sample_from_stereo`, `n_zone_new`, `n_instrument_new`, `n_sample_bank_new`. Each needs a
      0 check and callers that handle it. A refused allocation today writes through 0 inside the
      host ([architecture 001](../architecture/001-the-allocator-never-frees.md)).
- [ ] **SF2 records are parsed eagerly**, at about 6× their file size on the heap (a 40 MB igen
      chunk takes 240 MB). Reading fields lazily from the chunk would make it 0. Not urgent: real
      files carry KBs of records, and since 2.2.2 the walk has a work budget and every allocation
      is checked. Output must stay identical on the SF2 fixtures.
- [ ] **An SFZ `(ptr, len)` parse entry point.** `n_sfz_parse` takes a C string, so anything after
      an embedded NUL is silently dropped. Additive API.
- [ ] **Guard-page fuzz harnesses** (`guard_sf2.fcyr` / `guard_sfz.fcyr`). Harness-only, no `src/`
      change; the shipped parsers already pass. Needs a `cyr_mmap == -1` fallback for the agnos
      target. The 2.2.2 SF2 and SFZ bombs show why: inputs of about 600 bytes, which is what the
      current fuzzers feed, cannot build them.
- [ ] Small items:
      - `n_recorder_write` should be `#must_use`: an ignored error silently drops audio.
      - The Hann window still grows by `vec_push`, about 8 KB per stretch. Size it once, as the
        other stretch buffers are since 2.2.2.
      - `n_sfz_to_instrument` re-hashes a 4 KB joined name per region (257 ms for 20k regions).
        It is linear, but a pointer-identity fast path would remove it.
- [ ] Housekeeping:
      - `docs/port/12-vidya-port-template.md` is a template for porting *other* projects, and still
        describes a live `rust-old/`. Harmless here, but it is the file a future port would copy.
      - `docs/port/13-hisab-lib-template.md` shows bare `ERR_*` in its snippet. A warning was added
        in 2.0.2, but the snippet itself is still the wrong pattern to copy.

## 2.4.0 — SFZ articulation: exclusive groups and layered regions

The largest remaining gap between nidhi and SFZ playback. It needs a design ADR before any code.

- [ ] **SFZ `group=` / `off_by` exclusive classes, and layered regions.**
      - **What SFZ means:** `group=N` names an exclusive class that `off_by=N` silences (hi-hat
        choke), and every region matching a note sounds at once (mic positions, release layers).
      - **What nidhi does:** it plays one zone per note, and maps `group=` to round-robin, as the
        oracle did. Since [ADR 0012](../adr/0012-round-robin-plays.md), co-matching zones that
        share a `group=` alternate. A layered instrument plays one layer, and `off_by` is not
        parsed.
      - **Doing it right:**
        - parse `off_by`, and map `group=` to an exclusive class built on the existing choke
          mechanism;
        - drop `group=` as a round-robin source (`seq_length` / `seq_position` already carry SFZ
          round-robin);
        - make note_on start a voice for every matching zone, with the voice-allocation and steal
          rules extended to note groups.
      - This is a deliberate divergence from the oracle and changes what most multi-region SFZ
        instruments sound like, so it needs an ADR, fixtures for choke and layering, and a check
        that `note_on` stays allocation-free.

## 2.5.0 — streaming and bus routing

New capability, with additive API. No consumer is blocked on either today.

- [ ] **`NStreamReader` can stream for real.** shravan 2.12.4's `decode_reader` and
      `wav_stream_*` are genuinely incremental:
      - the input buffer stays about one chunk long;
      - chunks are exactly `chunk_frames`;
      - there is no 2^26 ceiling.

      On a 10 s file the first chunk costs 332 KB and 0.28 ms, against 10.3 MB and 6.1 ms today.
      The total heap still equals the decoded size, because the allocator never frees. What it
      takes:
      - an fd kept on the reader and closed at EOF, or a new close API;
      - channels taken from the HEADER event;
      - `total_frames` known only from the header until EOF (0 for pipes), which suspends the
        2.0.2 "derived from decoded data" invariant until the end;
      - the scrub and the [ADR 0010](../adr/0010-multichannel-loads-front-pair.md) front pair
        applied per chunk;
      - a pending queue for partial events.
- [ ] **`fill_buses_stereo`: real bus routing, not the oracle's stub.** This is the last unported
      public function from the Rust surface. Porting it verbatim would be worse than not porting
      it: the oracle accumulates the whole mix into `buses[0]` and ignores `output_bus`.
      `NZone_output_bus` is parsed from SFZ `output=`, inherited and stored, and read by nothing.
      That is the shape `volume_db` had before [ADR 0004](../adr/0004-apply-zone-volume-db.md).
      The fix: give the voice an `output_bus` at note-on and accumulate per bus. Additive API.

## 2.6.0 — oracle behaviours awaiting the maintainer's call

Each was faithfully inherited from the Rust oracle and found again in 2.2.2. The maintainer
deferred the first two then. Each is small, and each changes sound, so each needs an ADR.

- [ ] **A reverse-loop zone is silent unless it has a start offset.** `LOOP_REVERSE` starts the
      voice at `sample_offset` (0 by default), steps backwards, and ends on the first sample.
      Measured: one stereo frame from offset 0, and audio from offset 4000. The oracle does the
      same (`advance_position`, `LoopMode::Reverse`, and note-on's `position = sample_offset`), so
      starting at the end is a deliberate divergence. SFZ never sets this mode; only the API reaches
      it.
- [ ] **SF2 spec gaps:**
      - sampleModes 2 maps to Forward, where spec §8.1.2 says to treat it as no loop;
      - `original_pitch` 128-255 is used as the root, so 255 plays about 195 semitones low, where
        the spec says use 60;
      - `loop_end` past the sample's end is kept.
- [ ] **WSOLA amplifies output up to ~588x at the edges.** `normalize_by_window_sum` divides by
      `window_sum` only where it exceeds `1e-6`. At the output edges (one Hann frame, a tiny taper)
      the division amplifies instead of normalising: input max 0.97 gives output max 588.5, on
      every fixture tried. The oracle has the identical threshold and guard. Fixing it needs a
      listening check and new golden vectors as well as the ADR, because `tests/golden.tcyr` pins
      today's behaviour by sampling the interior on purpose.

## 2.x.x — unscheduled, each with what would schedule it

| Item | Schedule it when | Why it waits |
|---|---|---|
| **The 80 remaining unprefixed names** | a release with nothing else in it | [ADR 0005](../adr/0005-namespace-prefix-scope.md). 0 collisions measured: all 294 of nidhi's top-level names against the 9,579 in `lib/` (2.2.2), so the benefit is purely preventive. The naming questions are not mechanical (`nvf_*` already starts with `n` but not `n_`; `Sf2Shdr` -> `NSf2Shdr` is consistent and ugly), and a partially applied rename in a flat-namespace language is a live hazard, not a cosmetic one. The ADR lists the per-family decisions needed first. |
| **Voice-major block render** | a redesign with real invariant hoisting | The headline claim was **refuted by measurement**: alone, it makes 64-voice low-pass *worse* (17.734 -> 18.273 ms). The genuine 2.4x is only the many-idle-slots case (1 voice in 64: 441.7 -> 180.3 µs). 2.0.7 and 2.2.2 took most of the per-voice win by other means. Redo only with real invariant hoisting, a mandatory output-length guard, and mono coverage. |
| **Serde** | a consumer needs to persist a config | [ADR 0003](../adr/0003-serde-is-not-ported.md). A feature, not a repair. `NZone` has 34 fields against a 16-field derive guidance, and with no traits there is no `Deserialize` to derive, so every type needs a hand-written `from_json` and a round-trip test. |

## Not planned — decided, with the reason, so they are not re-opened blind

| Item | Decision | Why |
|---|---|---|
| **NaN clamping to a bound** | keep | [ADR 0002](../adr/0002-nan-clamps-to-the-bound.md). A NaN parameter yields a finite value at a clamp bound, where Rust propagated NaN. A NaN reaching the SVF latches the voice for its whole life. The "ignore non-finite input entirely" alternative is recorded there; revisit only if the hard-left-pan-on-NaN surprise is ever reported. |
| **4-accumulator `n_dot_raw`** | rejected in 2.2.2 | It makes WSOLA 1.44× faster, but summing in four lanes is not bit-exact. Dot results only pick an argmax, so output would differ only on a near-tie to the last ulp, but the project rule is playback accuracy over speed. |
| **PERF-03 layer 3** (Cauchy-Schwarz prune) | rejected | A numerical heuristic with a hand-tuned margin, not an exact transformation: it trades audio accuracy for speed against an explicit project rule. The two exact WSOLA layers delivered 4.02x without it. |
