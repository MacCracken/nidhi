# Changelog

## 2.0.7 — 2026-08-31

Performance. **Every change is bit-identical** — no audio output moves.

| | 2.0.6 | 2.0.7 | |
|---|---:|---:|---:|
| `wsola_1sec_2x` | 874.7 ms | **217.7 ms** | **4.02×** |
| `interpolation_stereo` | 170 ns | **60 ns** | **2.83×** |
| `interpolation_cubic` | 87 ns | **37 ns** | **2.35×** |
| `voice_count_scaling/64` | 21.761 µs | **13.245 µs** | **1.64×** |
| `fill_buffer_stereo/16` | 2.945 ms | **1.901 ms** | **1.55×** |
| `fill_buffer_stereo_filtered_8v` | 2.063 ms | **1.511 ms** | **1.37×** |

`fill_buffer_stereo/16` is now 1.901 ms against an 11.6 ms budget — **6.1× real-time headroom**,
up from 3.9×.

### How bit-identity was established

A render differential dumps **24,576 samples** as raw bit patterns across 8 configurations —
mono and stereo source, forward loop, crossfaded loop, low-pass, high-pass, pitched, key-tracked,
each over three blocks with a note-off partway. Captured before the first edit and re-compared
after every one; `cmp` clean throughout. The WSOLA path additionally has `tests/golden.tcyr`,
which asserts output *values* against the retired Rust oracle — the coverage 2.0.4 captured
specifically so a change like this could be made safely.

### Optimised

- **WSOLA correlation search → raw-pointer dot product** (`src/stretch.cyr`). ~88M element pairs
  per one-second stretch, every one going through `vec_get`'s two bounds tests, twice over. The
  callers have already proven both spans in range.
- **`prev` is now an index into `input`, not a copy.** It was a fresh `frame_size` vec built per
  frame — but the frame is a pure slice of `input`, and the loop guard already proves the whole
  span in range, so the copy allocated and memcpy'd for nothing: ~2.8 MB per stretch on an
  allocator that never reclaims.
- **Interpolation interior fast path** (`src/sample.cyr`). The four taps span `idx-1 .. idx+2`,
  so a single test — `idx - 1 >= 0 && idx + 2 < frames` — proves all of them in range, *including*
  for stereo, because the highest element touched is `(idx+2)*2+1 ≤ 2*frames-1` under exactly
  that condition. Eight per-tap helper calls (each with two bounds tests, three accessor calls,
  and `vec_get`'s own bounds test) collapse to eight raw loads. Zero-padding semantics stay on the
  edge path. ⚠ That test must stay exactly as written: an off-by-one is a silent OOB read.
- **Hermite coefficients are literal bit patterns** — they were rebuilt with two divides and
  three negations on every interpolated sample.
- **Filter key-tracking folded into `base_cutoff` at note-on.** Both inputs (the voice's note and
  its `fil_keytrack`) are written once and never mutated, so the render path was paying an
  `f64_pow` — an x87 ln+exp pair — per sample per voice for a constant. Multiplication order is
  unchanged, `(base × keytrack) × env × lfo`, which is why it stays bit-identical.
- **Non-allocating `n_instrument_find_first_zone`** for `note_on`, which previously called
  `find_zones` (allocating a vec of *every* match), took element 0, then scanned the zone list a
  second time comparing pointers to recover its index. Two O(Z) passes and ~1 KB of unreclaimed
  heap per note-on. `n_instrument_find_zones` is untouched — it is public API and tests use it.
  Deliberately *not* reused: `find_zone_rr` advances the round-robin counter and picks a group
  member, so substituting it would silently change zone selection for every grouped instrument.
- **`nvf_set_cutoff` literal hoists** — 0.49, 20.0 and the 0.5 hysteresis threshold were rebuilt
  with divides per sample per filtered voice.
- Widened one more `inst == 0` guard to `<= 0` in `n_engine_note_on`; negative error codes from
  `sf2_parse` / `n_load_wav` were still passing through it.

### Added — a benchmark that is meant to look bad

`fill_buffer_stereo_filtered_hp_8v` (1.797 ms) runs the same workload as the low-pass case
(1.511 ms) with `FILTER_HIGHPASS`. The **19 % penalty** is naad shipping an allocation-free SVF
core for low-pass *only*; high-pass, band-pass and notch voices still take the allocating
one-shot path at ~180 MB/s at 64 voices. Every other benchmark in the file sets `FILTER_LOWPASS`,
so this gap had no way to show up. Tracked for 2.1.0, gated on a naad change.

### Not done, deliberately

**PERF-03 layer 3** (the Cauchy–Schwarz early-exit prune) stays rejected. It is a numerical
heuristic with a hand-tuned margin, not an exact transformation, and it would trade audio
accuracy for speed against an explicit project rule. The two layers that *are* exact delivered
4.02× on their own.

### Quality
- **15 suites / 556 assertions / 0 failures**, fuzz 2/2, zero `#must_use` warnings.
- Before/after recorded in `bench-history.csv` per CLAUDE.md.

## 2.0.6 — 2026-08-31

Latent-hazard closeout. Every item is reachable by a **consumer** rather than by nidhi's own
code — and `dist/nidhi.cyr` is a public bundle, so "no in-tree caller does this" was never a
defence. Three ADRs, 556 assertions (up from 518).

### Fixed — `#derive(accessors)` re-opened states Rust made unrepresentable

This is the through-line for most of the release. Rust kept these fields **private**, so the bad
states could not be constructed; the derive generates a public setter for every one of them.

- **`n_effect_apply` dispatched on a mutable tag.** Rust's `EffectType` was an enum whose
  variant *carried* the state, so tag and payload could not disagree. Retagging a live
  `FX_REVERB` slot to `FX_CHORUS` handed a `Reverb*` to `effects_chorus_process_sample` — a wild
  read inside the audio callback. `NEffectSlot` now records a `state_type` at construction and
  dispatches on that; a mismatched slot passes through untouched. **Highest-severity item here.**
- **`NSampleRecorder_set_channels(r, 0)`** was an integer divide-by-zero in `n_recorder_frames`.
  Treated as mono.
- `NTimeStretcher_set_frame_size(s, -1000)` was already closed at the consumer in 2.0.3.

### Fixed — memory safety

- **`n_engine_fill_buffer(e, buf, n)` never checked `n` against `vec_len(buf)`.** Rust derived
  the frame count *from* `buffer.len()`, so over-asking was unrepresentable; the port took a
  separate length and trusted it, and `vec_set` past the end is `_vec_die` → `exit(1)` inside the
  audio callback. Both the mono and stereo fills now clamp.
- **A non-finite read position wrapped past every range guard.** `f64_to` is a raw `cvttsd2si`:
  `NaN` or an out-of-range position yields `i64::MIN`, and `idx - 1` then wraps to `i64::MAX`, so
  the "before the start" *and* "past the end" checks both read as in-range. `n_sample_read_cubic`
  and `n_sample_read_stereo_interpolated` reject non-finite up front.
- **The fourth unsaturated `f64_to`** (`n_adsr_config_is_default_sfz`) — an absurd `sample_rate`
  made the threshold negative, which *inverts* that predicate rather than merely widening it.

### Fixed — aliasing where Rust took by value

Same class as the `set_release_ms` fix in 2.0.2, now swept: `n_zone_with_adsr`,
`n_zone_with_filter_envelope` and `n_engine_set_adsr` stored the caller's `NAdsrConfig` pointer.
One config handed to several zones made them share mutable state, and a later
`NAdsrConfig_set_*` rewrote every one of them at once. All three now copy.

⚠ `tests/zone.tcyr` previously asserted `NZone_adsr(za) == cfg` — it was **pinning the aliasing
itself**. Replaced with value equality plus a proof that mutating the source config reaches
neither of two zones built from it.

### Fixed — `-0.0` broke the SFZ inheritance sentinel

`sfz_inh` decides "was this opcode explicitly set?" by comparing against the default as a raw
**bit pattern**, and `-0.0` is `0x8000…0`, not bit-equal to `+0.0`. The oracle compared with
`== 0.0`, where `-0.0` *is* equal. So `<global> volume=-6` + `<region> volume=-0` inherited
`-6.0` in Rust and stayed at `-0.0` here — and at the `has_ampeg` gate, `ampeg_attack=-0` made
the port wire a 1-sample-release ADSR the oracle never wired at all. `sfz_f64` now normalises
`-0.0` at the parse site: one fix for all 36 inheritance sites, and behaviour-preserving
downstream (same amplitude, same pan, same envelope).

### Decided — two open questions closed with ADRs

- **[ADR 0002](docs/adr/0002-nan-clamps-to-the-bound.md) — a NaN parameter clamps to a bound.**
  `f64_min`/`f64_max` are built on `f64_lt`/`f64_gt`, which return 0 for a NaN operand, so
  `n_zone_with_tune(z, NaN)` → `-12800` and `n_zone_with_pan(z, NaN)` → hard left, both finite,
  where Rust's `f32::clamp` propagates NaN. **Kept deliberately**: a NaN reaching the SVF latches
  the voice for its whole life, and nidhi stops non-finite audio at every other boundary already.
  Now pinned — finiteness *and* the specific bound.
- **[ADR 0003](docs/adr/0003-serde-is-not-ported.md) — serde is not ported.** The Rust crate
  derived `Serialize + Deserialize` on ~18 public types. **Four documents claimed the port
  carried this forward and none of it exists** — `grep '#derive' src/` is 100 % `accessors`. The
  2.0.3 audit found this independently in five of its six module groups. `CLAUDE.md`,
  `src/error.cyr` (which pointed readers at `zone.cyr` "for an example" that never had it),
  `docs/port/01-PLAN.md` D2 and `docs/port/16-serde-and-testing.md` are corrected. Implementing
  it is a feature for 2.1.0, not a repair: `NZone` has 32 fields against a 16-field guidance, and
  Cyrius has no traits, so there is no `Deserialize` to derive.

### Quality
- **15 suites / 556 assertions / 0 failures**, fuzz 2/2, zero `#must_use` warnings.

## 2.0.5 — 2026-08-31

Coverage backfill. Every Rust `#[test]` the 2.0.4 audit named as having no Cyrius counterpart now
has one, recovered from git history now that `rust-old/` is gone.

**518 assertions, up from 441.** Nothing failed on first run — these paths were correct, they
were simply never asserted, which is exactly what makes them cheap to break later.

### Added — envelope

- **`amp_envelope_zero_attack`.** `attack_samples=0, decay_samples=0` must settle at sustain.
  Not an edge case: this is the shape of the **default** config and of every SFZ region with no
  `ampeg` opcodes — the most common configuration in production, and it had no assertion
  anywhere.
- **`amp_envelope_smooth_release_from_mid_attack`.** Releasing part-way up the attack ramp must
  ramp *down* from the current level, not restart from 1.0 — otherwise the envelope jumps up on
  note-off, an audible click. Every existing release assertion released from a settled sustain,
  so this path was untested.

### Added — SFZ

- `parse_empty_file`, and `invalid_opcode_ignored` (unknown opcodes must be dropped silently
  without derailing the rest of the region).
- **`loop_mode_mapping`, all six oracle cases.** `no_loop` and `one_shot` are not recognised
  names in the mapper — they reach `OneShot` through the same default as an unknown token, which
  is exactly why the oracle asserted all six explicitly rather than trusting the fall-through.
- **`loop_start` / `loop_end` values** — the suite asserted the loop *mode* but never the
  numbers, on the path 2.0.2 rewrote to `sfz_u32`.
- **Wiring exercised by nothing in the repo**: `resonance` / `fil_resonance` (both spellings),
  `pitchlfo_freq` / `pitchlfo_depth`, `fillfo_freq` / `fillfo_depth`, and the `<curve>` header.

### Added — the oracle's maximal Zone builder chain

`rust-old/src/lib.rs`'s round-trip test built a Zone through **23 chained builders**, and
`docs/port/20-mod-core.md` called it "the single best fixture for Zone field parity" — while
citing it by *line number* rather than reproducing it, so it died with the oracle. Reconstructed
from git history in `tests/zone.tcyr`, with every field read back and matching re-checked
afterwards. **15 of the 23 builders had no direct assertion anywhere** before this.

### Changed — two tolerances restored

The port had flattened both onto the suite's general tolerance, weakening what they tested:

- **Hann window endpoints: 0.02 → `1e-6`.** The oracle used two different tolerances here on
  purpose — endpoints are *structurally* zero, the midpoint only approximately 1.0. A 0.02
  endpoint check passes on a window that never tapers, which is the entire property.
- **`trim_silence` value: 0.01 → `N_F32_EPSILON`.** Trim must copy the surviving frames
  untouched, so anything but an exact carry is a defect.

### Added — remaining partials

- **`invalid_ratio_returns_empty` was half-ported**: the four guards on `n_stretch` were
  asserted, the four on `n_stretch_ola` were not — and it is a separate function with its own
  copy of the guard prologue. Both halves now, plus `-Inf`, which neither side ever tested.
- `detect_onsets` on an empty sample and on a sample shorter than its window.

### Quality
- **15 suites / 518 assertions / 0 failures**, fuzz 2/2, zero `#must_use` warnings.
- Recovery from git history worked exactly as 2.0.4 documented. One wrinkle worth recording:
  under zsh, `git show $REV:rust-old/src/x.rs` silently mangles the path — `$REV:r` is parsed as
  a variable modifier. Use `${REV}:rust-old/...`.

## 2.0.4 — 2026-08-31

**The Rust oracle is retired.** `rust-old/` is gone from the working tree — 377 MB reclaimed,
repo 525 MB → 148 MB — after capturing the behaviour it pinned and recording the two things
about it that were not in git.

### Why this was safe

The 2.0.3 audit's verdict was "safe after pre-work", and the pre-work was smaller than it looked.
`rust-old/` was **git-tracked** (1002 files), so every `.rs` file, benchmark definition and
`#[test]` is recoverable forever with `git show <rev>:rust-old/src/<mod>.rs`. Only three things
genuinely needed doing first, and all three are done:

1. **`Cargo.lock` was untracked** (`.gitignore` excluded it) — the single unrecoverable file in
   the directory, and the only record of what "parity" was measured against. All 88 dependency
   versions are now in
   [`docs/port/oracle-build-identity.md`](docs/port/oracle-build-identity.md), along with the
   rustc identity (`1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6) that
   `rust-old/rust-toolchain.toml` never pinned. **The oracle linked naad 1.2.5** — a different
   major version from the naad the port links today, so any future "did Rust really do X?"
   question has to be answered against 1.2.5.
2. **`bench-history-rust.csv` was untracked** — a blanket `*.csv` in `.gitignore` was silently
   excluding it *and* the live `bench-history.csv`. Both are tracked now.
3. **Stretch output *values* were untested on both sides** — the audit's headline gap. All 13
   Rust and all 40 Cyrius stretch assertions checked only length, non-emptiness or finiteness, so
   a wholesale algorithmic defect that preserved output length passed everything.

### Added — golden vectors

`tests/golden.tcyr` (**43 assertions**) against `tests/golden-oracle.txt`, captured from a live
oracle build before deletion. Generated by a throwaway crate depending on `rust-old/` **by path**,
so the oracle itself was never modified (CLAUDE.md's rule held right up to its removal).

The fixture is `((i * 7) % 32) / 32` — values `k/32`, exactly representable in **both** f32 and
f64, so any mismatch is the algorithm and never the input.

| Vector | What it pins |
|---|---|
| `stretch_short` × 4 ratios | Full output values + lengths from the linear-interp resample path |
| WSOLA / OLA × 2 ratios | Interior values at index 2000 (fully-overlapped region) + total lengths |
| `round_ties` | `n = 1,3,5,7,9,11` at ratio 0.5 → **1,2,3,4,5,6**. Half-to-even would give 0,2,2,4,4,6, so this is a direct discriminator for the 2.0.3 `n_round_half_away` fix |
| `loop_points` | The full 10-candidate `detect_loop_points` ordering |

All 43 passed on the first run — including `detect_loop_points`, where the audit had flagged that
the f32→f64 mono downmix could permute near-ties. It does not.

### Recorded — an inherited defect, captured before the oracle went

`TimeStretcher::stretch` amplifies its output by up to **~588×** (measured: input max 0.97 →
output max 588.5, on every fixture tried). `normalize_by_window_sum` divides by `window_sum` only
where it exceeds `1e-6`, so at the output edges — one Hann frame, tiny taper — the division
amplifies instead of normalising. **The Cyrius port reproduces this faithfully** (identical
threshold and `>` guard), so it is an *inherited upstream defect, not a port defect*. Fixing it
is a deliberate divergence needing its own ADR; tracked for 2.1.0. The golden vectors above
deliberately sample the interior region so they measure the algorithm rather than this
amplification.

### Changed
- **CLAUDE.md's port conventions rewritten.** "Cross-check against `rust-old/`" is replaced by an
  ordered procedure: golden vectors first, then git history, then the recorded build identity.
  The "do not modify `rust-old/`" rule is retired with the directory.
- `docs/development/state.md`, `roadmap.md`, `docs/guides/getting-started.md`,
  `docs/port/01-PLAN.md` and `docs/benchmarks-rust-v-cyrius.md` updated — the last of those noted
  a `cd rust-old && cargo bench` reproduction step that no longer exists.
- Provenance comments in `src/*.cyr` and `tests/*.tcyr` ("Ports rust-old/src/X.rs") are left
  alone: they are accurate history and still resolve against git.
- **`CONTRIBUTING.md` rewritten for Cyrius.** It was still a pure Rust document — `cargo fmt`,
  `cargo clippy`, MSRV 1.89, `#[non_exhaustive]`, "feature-gate `naad` behind
  `#[cfg(feature = \"std\")]`" — orphaned since the port and actively misleading once the Rust
  tree was gone. Now documents the real toolchain, the flat-namespace and f64-bit-pattern rules,
  the zero-allocation render-path constraint, and the parity procedure.

### Quality
- **15 suites / 441 assertions / 0 failures** (up from 398), fuzz 2/2, zero `#must_use` warnings.

## 2.0.3 — 2026-08-31

Closes the loop-crossfade seam, the one item the 2.0.2 sweep confirmed but deferred because it
changes rendered audio and had no coverage to change it against.

### Fixed — loop crossfade never closed the seam

The port faithfully transcribed `rust-old/src/engine.rs:813-829`, which reads the fade-in source
**forward** from `loop_start`:

```
xfade_pos = loop_start + (xfade - dist)
```

`dist` counts down to 0 as the playhead reaches `loop_end`, so at the wrap this reads
`loop_start + xfade` — while the very next frame, now wrapped, plays `loop_start`. The seam ends
on a jump of `xfade` samples backwards through the material. **The crossfade attenuated the click
without ever closing it.** That formula belongs to a convention where the loop also wraps to
`loop_start + xfade`; the wrap target is plain `loop_start`, so the port inherited a formula from
one convention and a wrap point from the other.

The fade-in source now walks **backward** — `loop_start - dist` — because the material that must
follow `loop_end` is the material that *precedes* `loop_start`. At the wrap it reads exactly
`loop_start`, where the post-wrap playhead sits. The loop's period, and so its pitch, is
unchanged.

Two clamps, both load-bearing:
- **against the loop length** — a crossfade longer than the loop used to be live from frame 0,
  so the loop's own material was never heard at full level.
- **against `loop_start`** — there is no material before frame 0 and reads there return
  zero-padding, so an unclamped backward fade blends in **silence** and attenuates the loop by up
  to 50 %. The naive fix is wrong without this.

Where `loop_start < xfade` there is no pre-roll at all, and no crossfade can close a seam whose
endpoints differ — only moving the wrap point could, at the cost of the loop's pitch. That case
keeps the legacy forward blend: closure where it is achievable, never worse than 2.0.2 where it
is not. Disabling the fade there was measured and rejected — it was worse than shipping nothing.

⚠ **This changes rendered audio** for any zone with `crossfade_length > 0`, a forward or sustain
loop, and `loop_start >= crossfade_length`. A deliberate divergence from the oracle, recorded in
[ADR 0001](docs/adr/0001-loop-crossfade-seam.md). CLAUDE.md's "sample-accurate loop points and
crossfades" is what makes the oracle's behaviour a defect rather than a baseline.

### Added — the coverage that made this measurable

There was **no crossfade test and no crossfade benchmark anywhere in the repo** before this
release, which is why the defect survived the port, two parity audits, and a P-1 sweep.

`tests/engine.tcyr` now measures the largest frame-to-frame discontinuity across the wrap on a
400-frame linear ramp — a signal where a correct crossfade is *exactly* continuous. Values ×1e6:

| case | 2.0.2 | 2.0.3 |
|---|---:|---:|
| `xfade=20`, loop 100..200 | 22,999 | **4,000** |
| `xfade=0` (control) | 99,000 | 99,000 |
| `xfade=20`, loop 0..200 (no pre-roll) | 28,000 | 28,000 |
| `xfade=500` (longer than the 100-frame loop) | 19,602 | **1,000** |

The ramp's own per-frame step is ~2,500, so 4,000 is essentially a closed seam. The control and
the no-pre-roll case are pinned unchanged, so neither can regress silently.

### Fixed — two more parity divergences, found by re-reading the oracle

A full `rust-old/` port-coverage audit ran alongside this release (six modules, each
adversarially verified). It confirmed 293/328 public items ported and could not falsify a single
symbol-level claim — but it caught two real divergences that the 383-assertion suite could not
see, both by reading the Rust source rather than the tests:

- **`f64_round` is round-half-to-EVEN; Rust's `.round()` is half-away-from-zero.** Verified:
  Cyrius gives 0.5 → 0, 1.5 → 2, 2.5 → 2, 3.5 → 4. All three `.round()` sites in the oracle set a
  stretched buffer's target length, so any `input_len × ratio` landing exactly on .5 produced an
  output **one frame shorter** than Rust's. Now `n_round_half_away` in `src/f64_util.cyr`,
  computed from floor/ceil rather than `floor(x + 0.5)` — which is subtly wrong for
  `x = 0.49999999999999994`, where the addition rounds up to exactly 1.0.
- **Signed `inf` / `nan` were rejected by the 2.0.2 SFZ float validator.** `str_substr` takes an
  exclusive **end index**, not a length, so the validator sliced the wrong span whenever a sign
  was present. Unsigned `inf` worked only by coincidence (end == len with no sign). Introduced in
  2.0.2 and fixed here.

The audit also found that the 2.0.2 `group=300000000` regression test passed a `Str` where
`n_sfz_parse` takes a cstring, so it was never exercising the path it claimed to. Corrected.

### Quality
- **14 suites / 398 assertions / 0 failures**, up from 378. Fuzz 2/2.
- First ADR in the repo; `docs/adr/README.md`'s index was still "_No ADRs yet_".
- [`docs/development/roadmap.md`](docs/development/roadmap.md) rewritten: every item deferred by
  the 2.0.2 sweep and the 2.0.3 audit is now pinned to a release across the 2.0.x arc, replacing
  a stale v0.1.0-era milestone scaffold.

## 2.0.2 — 2026-08-31

**P-1 audit and repair sweep.** A seven-lens adversarial audit (security, memory safety, oracle
parity, numerics, error discipline, performance, hygiene) raised 54 findings; 46 survived
independent refutation. This release fixes the security, memory-safety and correctness tier.

Two of them could kill the host process from a small input file, and one made most WAVs silent.

### Fixed — security

- **SF2 allocation bomb (`src/sf2.cyr`).** `preset → pbag → ibag` was an un-memoized triple loop
  over three attacker-declared u16 index fields, re-converting the *same* sample's PCM once per
  triple, so memory grew as the **cube** of file size. A **7,322-byte** file reached
  `vec: alloc failed → syscall(60,1)` — an unconditional `exit(1)` of the host (dhvani/shruti),
  with no catchable error. Now memoized per `shdr` **and** capped at 65,536 zones; both halves
  are load-bearing. Verified: the same file that killed the old binary under a 1 GB limit now
  completes with a 1-entry bank and 9 MB of heap.
- **Unbounded read loop (`src/io.cyr`).** `alloc`'s 2 GiB ceiling rejects with `>` not `>=`, so
  the doubling chain reached 2^32, `alloc` returned 0, and `memcpy(0, buf, 2147483648)`
  segfaulted. **No large file was needed** — `n_read_file("/dev/zero", …)` reached it, and
  `n_stream_reader_open` is public, steerable by an SFZ `default_path` + `sample=` pair.
- **SFZ numeric parsing (`src/sfz.cyr`).** Integer opcodes accumulated into an i64 and wrapped
  silently: `group=9223372036854775808` became `i64::MIN` and reached `vec_get` as a negative
  index — `exit(1)` from a **47-byte** file. Overflow is now rejected (or saturated where Rust's
  `parse::<usize>()` genuinely accepts the value), `sfz_u8` rejects negatives, and `group` /
  `seq_position` / loop points / `offset` / `end` / `tune` / `transpose` parse at the oracle's
  own widths via new `sfz_u32` / `sfz_i32`.
- **Group index bound (`src/instrument.cyr`).** Round-robin groups index `rr_counters` directly,
  so the group number doubled as an allocation size. `group=300000000` — a valid u32 the oracle
  accepts — asked for ~4 GB and hit `vec: capacity overflow`. Groups now cap at 4096; above that
  a zone imports as ungrouped.

### Fixed — memory safety

- **The render path allocated on every frame.** `n_engine_next_sample_stereo` called `alloc(16)`
  three times per frame and `n_engine_next_sample` a fourth: **48 B/frame stereo = 2,116,800 B/s**
  at 44.1 kHz, never reclaimed, because `lib/alloc.cyr` is a bump allocator with a no-op free.
  The endgame was not a leak but memory *unsafety* — once `alloc` returns 0 the render path
  `store64`s through a null pointer inside the audio callback. Now four per-engine slots
  allocated once in `n_engine_new`. `tests/engine.tcyr` asserts an `alloc_used()` delta of
  **exactly 0** across 20 blocks at 8 and 64 voices, filtered and unfiltered.
- **Allocating SVF on the hot path.** Low-pass voices now use naad's
  `filter_svf_process_sample_lowpass`; the one-shot API allocated an `SvfOutput` per channel per
  voice per sample (22.6 MB/s at 8 voices, 180 MB/s at 64). naad's own header says the hot path
  must use this form. Integrator writes are identical, so filter state evolves bit-for-bit.
- **Reverb allocated per sample** (`src/effect_chain.cyr`) — 1,411,200 B/s. Now routed through
  `reverb_process_core` with one reused scratch, which is exactly what the wrapper did anyway.
- **Recorder buffer aliasing** (`src/capture.cyr`). Rust *moved* the buffer out of `finish()`;
  the port aliased it, so `finish()` then `clear()` left the Sample claiming N frames over an
  empty vec (`exit(1)` on the next read) — and the non-crashing shape was worse, silently playing
  the *new* take's audio under the old frame count. `finish()` now consumes the recording.
- **Stale zone index across a live instrument swap** (`src/engine.cyr`). Hot-swapping an 8-zone
  patch to a 1-zone patch on a sustaining note indexed the new vec out of bounds → `exit(1)`
  mid-render. The voice now deactivates, mirroring the existing sample guard.
- **Voice-count desync** — naad clamps `max_voices` to ≥1, nidhi did not, so `n_engine_new(0, sr)`
  built 0 nidhi voices against naad's 1 and the first `note_on` indexed an empty vec.
- **Negative error codes laundered into pointers.** `n_load_wav` and `sf2_parse` return negative
  codes; `== 0` guards let them through into `NSample_frames(-5)` — a wild read in the audio
  callback. `n_sample_bank_add` now rejects them at the trust boundary, and the `== 0` guards in
  `engine.cyr` / `effect_chain.cyr` became `<= 0`.

### Fixed — correctness

- **Integer-PCM WAVs decoded to silence** (carried over from 2.0.1's known list). nidhi never
  called `shravan_init_constants()`, so all 17 of shravan's f64 constants were the bit pattern 0
  — which is `+0.0`. The little-endian converters *multiply* by them, so every 8/16/24/32-bit
  integer PCM file decoded to pure silence; the big-endian converters *divide* by them, so an
  AIFF returned a **successful** Sample whose every frame was ±inf/NaN. A new idempotent
  `n_init()` runs from every public entry point. `tests/io.tcyr` now asserts decoded *values*
  for I8/I16/I24/I32 — the old fixtures were all `PCM_F32`, the one format needing no constant,
  which is exactly why this was invisible.
- **The streaming reader truncated every file over ~4 KB to its first 1013 frames.** It fed
  shravan in 4096-byte slices, but that decoder is buffer-then-decode-once and `wav_decode`
  clamps the declared size to what is present — so the first slice decoded as a *complete* WAV
  and every later feed was ignored. `total_frames` stayed correct, so the reader disagreed with
  itself. Now feeds the whole remainder.
- **`n_normalize_peak` amplified near-silence** (`src/capture.cyr`). Rust scales only when
  `peak > 1e-10`; naad guards only `peak > 0`. A 1e-12 peak was amplified ×1e12, and a
  *denormal* peak made `1.0/peak` overflow to **+inf**, writing non-finite values into sample
  data — an inf/NaN injection site inside nidhi with no foreign decoder involved.
- **Non-finite samples now stopped at the decode boundary.** One NaN frame latches the SVF and
  poisons a voice for its whole life.
- **SFZ float opcodes accepted trailing garbage.** `f64_parse_ok` succeeds on a prefix, so
  `cutoff=1000Hz` low-passed every note at 1 kHz where the oracle disables the filter, and
  `ampeg_attack=10ms` became `attack_samples=441000` — a **ten-second fade-in on every note**
  where the oracle applies no envelope at all. Now whole-token validated, still accepting
  `.5`, `5.`, `+3`, `1e1`, `inf`, `NaN` as Rust does.
- **`f64_to` overflow inverted long envelopes.** It is a raw `cvttsd2si` returning `i64::MIN`
  out of range, which collapses to 0 — so an "infinite" attack/release became *instantaneous*, a
  click. Saturated to u32 range at all three live sites.
- **Sub-1 Hz sample rates hung voices forever.** A denormal `sample_rate` (most plausibly a raw
  i64 `44100` passed where an f64 bit pattern was expected, which reads as 2.18e-319) made the
  release time `+inf`; naad accepts that, and the voice never reached IDLE. Routed into the
  existing fallback — *without* clamping the input, so the oracle-parity behaviour that
  `tests/envelope.tcyr` pins for finite rates is unchanged.
- **Negative velocity produced NaN** — `f64_sqrt` of a negative. Rust's `u8` made it
  unrepresentable; the i64 port did not. Lower bound clamped; the upper bound deliberately is
  not, since u8 legitimately spans 0..255.
- **Negative `frame_size` hung `n_stretch` forever** — `out_pos` marched negative and the sole
  loop exit was never reachable.
- **`n_engine_set_release_ms` mutated a shared config in place**, rewriting the caller's own
  object and every zone handed the same pointer. Rust's `set_adsr` takes the config by value.

### Changed — BREAKING

- **`ERR_*` → `NIDHI_ERR_*`.** In the flat concatenated bundle a bare `ERR_NONE` collided three
  ways — nidhi, `lib/goonj.cyr`, and shravan's `enum ShravanErr`. All were `0`, so nothing
  misbehaved, which is precisely the hazard: Cyrius warns on a duplicate `fn` and is **silent**
  on a duplicate `var`, so the clash stays invisible until one side renumbers. naad escaped the
  same trap in 2.2.0. `docs/port/26-mod-io-bench.md` specified `NIDHI_ERR_IMPORT` from the start.
  Consumers must update; dhvani is coordinated separately and shruti is not yet ported.
- **`src/sf2.cyr` no longer returns raw magic literals.** All 15 (`-1`..`-3`, `-6`..`-15`) now
  return `NIDHI_ERR_IMPORT`. Three were bit-identical to codes from a *different* taxonomy, so
  handing a WAV to the SF2 importer reported "invalid parameter" and a truncated file reported
  "sample not found". rust-old returns `ImportError` for all 19 corresponding sites.

### Performance

- `fill_buffer_stereo_filtered_8v` **2.247 → ~2.0 ms (−12 %)**, from the alloc-free low-pass
  path. Cumulative since the 6.3.34 baseline: 2.592 → 2.0 ms, ~22 %.
- `n_trim_silence` now short-circuits both scans. The oracle uses `.find()` / `.rev().find()`;
  the port kept walking the whole buffer after finding its frame.
- Everything else is within run-to-run noise. See `BENCHMARKS.md` and `bench-history.csv`.

### Quality
- **14 suites / 378 assertions / 0 failures**, up from 313 — the new coverage is concentrated
  exactly where the defects were invisible: `io.tcyr` 18 → 48, `sfz.tcyr` 63 → 90,
  `engine.tcyr` 29 → 37.
- **Zero `#must_use` warnings**, down from 28 after the four new annotations landed.
- Fuzz 2/2, `cyrius distlib --check` current.
- `error.cyr` and `f64_util.cyr` are now included by every test/bench/fuzz file, as the
  documented module-order convention always required.

### Fixed — CI
- `.github/workflows/ci.yml` and `release.yml` built `src/main.cyr`, a binary-project scaffold
  path that has never existed here — CI was red. Both now derive `[build].entry` / `[build].output`
  from `cyrius.cyml`, the same way they already derive the toolchain pin.
- `release.yml`'s changelog extraction required a **bracketed** heading (`^## \[$TAG\]`), which
  no heading in this file has ever used, so every release fell back to "No changelog entry". It
  also interpolated the tag into a regex, where a version's dots are metacharacters. It now
  compares the heading's version token literally and accepts either form.
- `release.yml` gained a `cyrius distlib --check` gate so a release cannot ship a `dist/nidhi.cyr`
  that disagrees with `src/`.

### Deferred
Real but out of scope for a patch release: the voice-major block render (measured *worse* than
frame-major at ≥8 voices without invariant hoisting), the streaming-reader restructure, WSOLA's
cross-correlation rewrite, the loop-crossfade seam (`xfade_pos` cannot close the loop, and
`crossfade_length` is unclamped against the loop length — both character-identical to rust-old,
both output-changing, and **there is no crossfade test or benchmark today**), per-note-on
allocation, `fill_buses_stereo`, and the `n_`/`N` prefix sweep over the remaining 100 top-level
names (currently zero measured collisions).

## 2.0.1 — 2026-08-31

Toolchain and dependency catch-up. **No functional change to nidhi's own behaviour** — the only
source edits are five renames forced by naad's namespace migration.

### Changed
- **Cyrius toolchain 6.3.36 → 6.5.36** (`cyrius.cyml [package].cyrius`, the single source of
  truth; CI reads the pin rather than hardcoding it)
- **naad 2.1.0 → 2.2.2**, **shravan 2.5.12 → 2.8.0**, **hisab 2.6.7 → 2.11.2**. Transitive:
  goonj 2.0.0 → 2.0.4, sankoch 1.0.0 → 2.7.10, sakshi 2.4.2 → 2.4.11.
- **naad namespace migration** — naad 2.1.1/2.2.0 moved six bare-lowercase helpers onto the
  `naad_` prefix. nidhi's five call sites follow; the function bodies are byte-identical, so
  output is bit-stable:
  ```
  normalize         -> naad_normalize          src/capture.cyr:136
  rms               -> naad_rms                src/capture.cyr:144
  peak              -> naad_peak               tests/capture.tcyr:81,128
  rms               -> naad_rms                tests/capture.tcyr:88
  db_to_amplitude   -> naad_db_to_amplitude    tests/naad_link.tcyr:13
  amplitude_to_db   -> naad_amplitude_to_db    tests/naad_link.tcyr:14
  ```
- `bump-version.sh` no longer edits a `Cargo.toml` — the file has not existed since the Cyrius
  port, and `set -e` made the script abort on it. VERSION is the sole source of truth
  (`cyrius.cyml` derives it via `${file:VERSION}`); the script now validates the SemVer argument
  and warns if that derivation is ever inlined.
- `docs/development/state.md` refreshed — it still described the 0.1.0 `cyrius port` scaffold.

### Fixed
- **Three undefined symbols resolved**: `mutex_new` / `mutex_lock` / `mutex_unlock` were
  referenced by the vendored bundles with nothing supplying them. The 6.5.36 stdlib snapshot adds
  `lib/sync.cyr` (plus `thread.cyr`, `thread_local.cyr`, `mmap.cyr`, `callback.cyr`), so the
  build now has **zero** undefined-function warnings, down from three.

### Not changed, and why — naad's headline rename does not apply
naad 2.2.0's `FILTER_* → NAAD_FILTER_*` rename names nidhi explicitly, because nidhi defines
`FILTER_LOWPASS..FILTER_NOTCH = 0..3` with values identical to naad's. **naad renamed its own
constants to de-collide, so nidhi needs no edit** — nidhi never consumed naad's `FILTER_*`, and
`src/zone.cyr:30-33` keeps its definitions. Likewise `ERR_* → NAAD_ERR_*` (2.1.3) and
`VOICE_* → NAAD_VOICE_*`: nidhi's `ERR_*` in `src/error.cyr` are its own.

nidhi references none of naad's behaviour-changed surfaces — `convolution`, `fit_polynomial`,
`tuning_note_name`, `ambisonics`, `binaural`, `panning`, `chromagram` — and none of hisab's public
API. Every other naad function nidhi calls is bit-identical between 2.1.0 and 2.2.2, and struct
layouts and enum values are unchanged, so the derive-generated accessors are safe.

### Quality
- **14 suites / 313 assertions / 0 failures**, identical per-suite counts to 2.0.0
- **Fuzz**: 2 harnesses, 0 crashes · **`cyrius audit`** exits 0 · `dist/nidhi.cyr` restamped v2.0.1
- **Benchmarks** re-run on 6.5.36 (`bench-history.csv`, `BENCHMARKS.md`). Everything is within
  run-to-run noise of the 6.3.34 baseline except `fill_buffer_stereo_filtered_8v`, **~12–13 %
  faster** across three runs (2.247 / 2.289 / 2.313 ms vs 2.592 ms) — the path that leans hardest
  on naad's SVF. Note that 6.5.19 reworked `lib/bench.cyr` to subtract a calibrated clock-read
  floor from every sample; at nidhi's batch sizes that is ≤0.3 ns/op, so the two series remain
  comparable, but sub-200 ns rows are dominated by timer jitter and should be read as indicative.

### Known — pre-existing, not introduced here
- **Integer-PCM WAVs decode to silence.** nidhi never calls `shravan_init_constants()`, so
  shravan's `F64_RCP_128` / `F64_RCP_32768` / `F64_RCP_8388608` / `F64_RCP_2147483648` stay at
  their `0` initialiser — which as an f64 bit pattern is `+0.0`. Every 8/16/24/32-bit integer PCM
  file therefore scales to zero; only `PCM_F32` decodes correctly. `tests/io.tcyr` does not catch
  it because every fixture encodes `PCM_F32`. Verified identical in shravan 2.5.12 and 2.8.0, so
  this predates the upgrade — but it is a real defect and should be fixed on its own, with
  integer-PCM coverage added to `tests/io.tcyr`.
- **`ERR_NONE` collides three ways** — `src/error.cyr:18`, `lib/goonj.cyr:16`, and
  `lib/shravan.cyr:82` (`enum ShravanErr`). All three are `0`, so nothing misbehaves, and Cyrius
  warns on a duplicate `fn` but not a duplicate `var`. This is exactly the invisible-until-one-
  side-renumbers hazard naad escaped in 2.2.0. Renaming nidhi's public error constants to the
  `N_` convention is breaking, so it is deferred to the next minor.
- **`.github/workflows/ci.yml` builds `src/main.cyr`**, a scaffold path that does not exist — the
  entry is `programs/smoke.cyr`.
- **Release notes never extract a changelog body.** `.github/workflows/release.yml:61` awks for
  `^## \[$TAG\]` — a *bracketed* heading — but no heading in this file has ever used brackets, so
  every release has silently fallen back to "No changelog entry for $TAG." Bracketing only this
  entry would be worse, not better: with no later bracketed heading the awk never hits its exit
  and would paste the entire file into the 2.0.1 release body (verified: 216 lines). The real fix
  is the awk pattern, not the headings.

## 2.0.0 — 2026-07-03

Full Rust → **Cyrius** port (toolchain 6.3.36). Major version marking the rewrite (and aligning
with the naad/shravan/hisab 2.x ecosystem); supersedes the Rust 1.1.0 line below. The Rust 1.1.0 source is preserved under
`rust-old/` as the parity oracle. All 14 modules ported for **feature-set parity**, leaning on
the converted **naad** (DSP/filters/LFOs/effects/voice management), **shravan** (WAV codecs), and
**hisab** (math) Cyrius libraries.

### Ported
- `error` (integer codes), `f64_util`, `loop_mode`, `envelope` (naad ADSR), `zone` (32 fields +
  velocity curves + `matches`/`playback_ratio`), `sample` (cubic-hermite interp, energy onset
  detection, `SampleBank`), `instrument` (find_zones + round-robin), `capture` (recorder, trim,
  loop detection), `stretch` (WSOLA/OLA), `effect_chain` (5-slot naad effects), `io` (WAV
  load/stream via shravan), `sf2` (RIFF/SoundFont binary parser), `sfz` (40+ opcode text parser),
  `engine` (voice mgmt + per-sample render loop over naad SVF/LFO/smoother/VoiceManager)
- Samples/floats are f64 (Cyrius has no f32); `#derive(Serialize)`-ready config structs; symbols
  `n_`/`N`-prefixed for the flat bundle namespace

### Quality
- **14 test suites, ~327 assertions, 0 failures** (`cyrius test`)
- Adversarial parity audit vs `rust-old/` (2 passes): fixed a sub-1.0-sample-rate envelope
  divergence, SFZ integer-parse strictness (u8 `>255` / negative-unsigned / leading `+`), an SF2
  malformed-sub-chunk error path, and made capture's loop-point sort stable
- **Benchmarks** — 7 Criterion benchmarks reproduced in `tests/nidhi.bcyr` (`BENCHMARKS.md`,
  `bench-history.csv`) for Rust-vs-Cyrius comparison
- **Fuzz** — `fuzz/fuzz_sf2.fcyr` + `fuzz/fuzz_sfz.fcyr` never-crash harnesses (6000 mutated/random
  inputs, 0 crashes)
- `dist/nidhi.cyr` distributable bundle via `cyrius distlib`

## 1.1.0 — 2026-03-28

Performance + real-time safety release. Zero-allocation render path, block-based voice rendering, filter caching, denormal protection, and SIMD infrastructure.

### Performance
- **Block-based voice rendering** — `fill_buffer_stereo` now renders each voice for the entire block into a pre-allocated scratch buffer, then accumulates into output. ~2.9x speedup for single-voice workloads, ~1.2x for 16 voices
- **Filter coefficient caching** — epsilon check on cutoff skips expensive `set_params()` when cutoff hasn't changed meaningfully (< 0.5 Hz). 3.4x speedup on filtered voices
- **Parameter smoothing** — per-voice `naad::smoothing::ParamSmoother` on filter cutoff modulation for click-free filter changes (std only)
- **SIMD stereo mixing** — SSE2 (x86_64) and NEON (aarch64) buffer accumulation behind `simd` feature gate, with scalar fallback
- **SIMD cubic Hermite interpolation** — SSE-accelerated stereo interpolation computes both L/R channels in a single SIMD pass (behind `simd` feature gate)
- **Pre-allocated scratch buffer** — engine allocates a reusable stereo scratch buffer at construction, eliminating render-path heap allocation

### Real-time Safety
- **Denormal flushing** — `flush_denormal()` applied to no_std filter feedback paths and envelope release ramp to prevent 10–100x slowdowns on x86
- **Removed per-sample Vec allocation** in `fill_buses_stereo` — eliminated `Vec::new()` that was called every sample frame

### Bug Fixes
- **Fixed infinite loop** in `detect_onsets()` when sample has ≤ 3 frames (hop became 0)
- **Fixed integer overflow** in SF2 chunk iterator — crafted SF2 with large chunk size could cause wraparound and infinite loop
- **Fixed `stretch()`/`stretch_ola()`** — ratio ≤ 0, NaN, or infinity now returns empty instead of producing inf/NaN

### Quality
- **Benchmark suite** — 7 Criterion benchmarks: voice count scaling, block vs per-sample buffer fill, cubic/stereo interpolation, filtered rendering, WSOLA throughput
- Added `#[must_use]` on 10 accessors/constructors across 5 modules
- Added `#[inline]` on 9 hot-path render functions and accessors
- **117 unit tests + 4 doc-tests** (up from 114)
- New `simd` feature flag for SIMD-accelerated mixing and interpolation

## 1.0.1 — 2026-03-28

### Changed
- **Replace hound with shravan** for WAV I/O — shravan provides broader codec support (WAV, FLAC, AIFF, Ogg, MP3, Opus), streaming decoding, and PCM format conversion
- `StreamingWavReader` now uses shravan's `WavStreamDecoder` for chunked decoding

## 1.0.0 — 2026-03-28

Stable release. Full-featured sample playback engine for AGNOS.

### Engine
- **Polyphonic playback** with configurable voice count and cubic Hermite interpolation
- **Voice management** via `naad::VoiceManager` (std) with hand-rolled fallback (no_std)
- **Steal modes**: Oldest, Quietest, Lowest, None (`StealMode` enum)
- **Poly modes**: Poly, Mono, Legato (`PolyMode` enum)
- **Choke groups**: Voices in the same group silence each other on note-on
- **Per-note expression**: `apply_pitch_bend()`, `apply_pressure()`, `apply_brightness()`
- **Pitch bend range** config (default ±2 semitones)
- **Multi-output routing**: Per-zone bus assignment, `fill_buses_stereo()`

### Zones
- **Key/velocity mapping** with full MIDI range, round-robin groups
- **Root note + tuning** (cents, transpose support)
- **Volume, pan** (constant-power stereo)
- **Velocity curves**: Linear, Convex, Concave, Switch
- **Filter**: SVF (LP/HP/BP/Notch) via naad with true stereo, resonance, velocity tracking, key tracking
- **Filter envelope**: Per-zone `fileg_*` config, modulates cutoff per-sample
- **Per-zone ADSR**: Overrides engine default, wired from SFZ `ampeg_*` opcodes
- **Pitch LFO + Filter LFO**: Per-voice via naad, from zone config
- **Loop modes**: OneShot, Forward, PingPong, Reverse, LoopSustain (release exits loop)
- **Crossfade loops**: Configurable linear blend at loop boundary
- **Sample offset/end**: Partial playback within a sample
- **Time-stretch ratio**: Per-zone config (0.25x–4.0x)
- **Output bus**: Per-zone routing to main or aux buses

### Envelopes
- **AmpEnvelope**: Wraps `naad::envelope::Adsr` (std) or built-in linear ADSR (no_std)
- **Smooth release** from any envelope level
- **AdsrConfig**: Sample-based config with `from_seconds()` convenience

### SFZ Import
- **Parser**: `<global>`, `<group>`, `<region>`, `<control>`, `<curve>` headers
- **40+ opcodes**: sample, key ranges, velocity, pitch_keycenter, tune, transpose, volume, pan, loop modes, filter (cutoff, resonance, fil_type, fil_veltrack), envelopes (ampeg_*, fileg_*), LFOs (pitchlfo_*, fillfo_*), fil_keytrack, offset, end, output
- **Note-name parsing**: C-1 through G9 with sharps/flats
- **`key` shorthand**: Sets lokey=hikey=pitch_keycenter
- **`<control> default_path`**: Prepends path to all sample filenames
- **SFZ v2**: `#include` directive collection, `_onccN` CC modulation parsing
- **Inheritance**: Global → group → region with correct override semantics

### SF2/SoundFont Import
- **RIFF binary parser**: Pure `&[u8]` parsing, no_std compatible
- **Preset/instrument/zone chain** resolution with key/velocity range masking
- **PCM16→f32** sample data extraction
- **Loop mode mapping**: SF2 sampleModes → nidhi LoopMode (including mode 3 → LoopSustain)
- **Returns nidhi-native types**: `(Vec<Sf2Preset>, Vec<Instrument>, SampleBank)`

### Sample Capture
- **SampleRecorder**: Accumulate `&[f32]` audio buffers into a `Sample`
- **Auto-trim**: `trim_silence()` removes leading/trailing silence
- **Normalize**: `normalize_peak()` (0 dB) and `normalize_rms()` (target RMS)
- **Loop detection**: `detect_loop_points()` via zero-crossing + cross-correlation scoring
- **Onset detection**: `Sample::detect_onsets()` for REX-style slice points

### Effects
- **EffectChain**: Up to 5 serial slots routing through naad effects
- **Effect types**: Reverb, Delay, Chorus, Compressor, Limiter
- **Per-slot bypass** and wet/dry mix

### File I/O (`io` feature)
- **WAV loading**: `load_wav()`, `load_wav_from_memory()` via shravan
- **Streaming**: `StreamingWavReader` for chunked reading of large instruments
- Supports 8/16/24-bit integer and 32-bit float WAV

### Time-Stretching
- **WSOLA**: Waveform Similarity Overlap-Add with cross-correlation splice points
- **OLA**: Simple Overlap-Add for speech/mono
- **TimeStretcher**: Offline processing with configurable frame size and overlap

### Quality
- **114 unit tests + 4 doc-tests**
- **Serde roundtrip tests** for all public types
- **Send + Sync** assertions for all public types
- **`#[must_use]`** on all accessors, **`#[non_exhaustive]`** on all public enums
- **Fuzz targets** for SFZ and SF2 parsers (libfuzzer-sys)
- **no_std + alloc** support with `std` as default feature
