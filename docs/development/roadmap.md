# nidhi — Roadmap

> Sequencing: what ships, in what order, against what gates. State lives in
> [`state.md`](state.md); this file is the plan.

Every item below traces to a specific finding from the 2.0.2 P-1 sweep or the 2.0.3 `rust-old/`
parity audit, both recorded in [`CHANGELOG.md`](../../CHANGELOG.md). Items are pinned to a
release, not left as a wishlist.

## Shipped

| Release | Theme |
|---|---|
| **2.0.0** (2026-07-03) | Rust → Cyrius port, all 14 modules |
| **2.0.1** (2026-08-31) | Toolchain 6.5.36 + dependency catch-up (naad 2.2.2 / shravan 2.8.0 / hisab 2.11.2) |
| **2.0.2** (2026-08-31) | P-1 audit: security, memory safety, correctness. `ERR_* → NIDHI_ERR_*` |
| **2.0.3** (2026-08-31) | Loop-crossfade seam ([ADR 0001](../adr/0001-loop-crossfade-seam.md)); round-half-away parity; signed `inf`/`nan` |
| **2.0.4** (2026-08-31) | **Rust oracle retired.** Golden vectors captured, build identity recorded, `rust-old/` removed (377 MB) |

---

## 2.0.5 — Coverage backfill

The golden vectors landed in 2.0.4 closed the largest gap (stretch output *values*). These are
the rest of the Rust `#[test]` cases with no Cyrius counterpart, from the 2.0.3 audit.

- [ ] `amp_envelope_zero_attack` — attack_samples=0, decay_samples=0 must reach sustain. This is
      the shape of the **default** config and of every SFZ region with no `ampeg` opcodes: the
      most common configuration in production and the least tested.
- [ ] `amp_envelope_smooth_release_from_mid_attack` — release entered part-way through the attack
      ramp must start the down-ramp from the *current* level, not from 1.0. A click if it
      regresses. `tests/envelope.tcyr` only ever releases from a settled sustain.
- [ ] SFZ: `parse_empty_file`, `loop_mode_mapping` (6 assertions), `invalid_opcode_ignored`,
      and `loop_start`/`loop_end` **values** (the path 2.0.2 rewrote).
- [ ] The three SFZ wiring paths exercised by nothing in the repo: `fillfo_*`, `pitchlfo_*`,
      `<curve>` header. Plus `resonance` / `fil_resonance`.
- [ ] Restore the two strict tolerances the audit flagged: Hann endpoints at `1e-6`,
      `trim_silence` at `N_F32_EPSILON` (already at `src/f64_util.cyr:16`).
- [ ] The maximal `Zone` builder chain — 10 of 23 `n_zone_with_*` builders have no direct
      assertion anywhere. The oracle's own fixture is recoverable from git history.

## 2.0.6 — Latent-hazard closeout

The 2.0.3 audit's behavioural hazards. None is currently reachable through nidhi's own code;
all are reachable by a consumer, and `dist/nidhi.cyr` is a public bundle.

- [ ] **`n_effect_apply` dispatches on the slot tag; Rust matched on the state variant.**
      `NEffectSlot_set_effect_type(slot, FX_CHORUS)` on a slot built as `FX_REVERB` hands a
      `Reverb*` to `effects_chorus_process_sample` — a wild read in the audio callback. Rust's
      variant match made this passthrough-safe. Highest-severity item in this release.
- [ ] **`#derive(accessors)` re-opens states Rust made unrepresentable.** Every `Zone` /
      `SampleRecorder` / `TimeStretcher` field is private in Rust; the derive generates public
      setters that bypass every clamp. `NZone_set_pan(z, 50.0)` reads back 50.0;
      `NSampleRecorder_set_channels(r, 0)` divides by zero at `src/capture.cyr:42`;
      `NTimeStretcher_set_frame_size(s, -1000)` reaches the non-terminating loop the 2.0.2
      builder guard only closed at the builder.
- [ ] **NaN clamps invert.** `f64_min`/`f64_max` are built on `f64_lt`/`f64_gt`, which return 0
      for a NaN operand, so `clamp(NaN)` yields the *bound* where Rust's `f32::clamp` yields NaN.
      Confirmed: `n_zone_with_tune(z, NaN)` → −12800; `n_zone_with_pan(z, NaN)` → hard left. The
      port is *safer*; it is also undocumented and untested. Decide and pin.
- [ ] **`-0.0` breaks the SFZ inheritance sentinel.** `sfz_inh` compares raw bit patterns;
      the oracle used `== 0.0`, which `-0.0` satisfies. `<global> volume=-6` + `<region>
      volume=-0` gives −6.0 in Rust and −0.0 here. Audible.
- [ ] **`n_engine_fill_buffer(e, buf, n)` never checks `n` against `vec_len(buf)`** → `_vec_die`
      → `exit(1)` inside the audio callback. Rust derived frame count from `buffer.len()`.
- [ ] Four unsaturated `f64_to` sites the 2.0.2 sweep missed: `src/envelope.cyr:73`,
      `src/sample.cyr:123`, `:146`, `src/stretch.cyr:192`. (CHANGELOG 2.0.2 claims "all three
      live sites" — the audit found four more.)
- [ ] Remaining aliasing-where-Rust-took-by-value: `n_zone_with_adsr`,
      `n_zone_with_filter_envelope`, `n_engine_set_adsr`, and `n_instrument_add_zone` writing
      `NZone_set_group` through the caller's pointer. Same class 2.0.2 fixed for
      `set_release_ms`.
- [ ] **Resolve the serde decision.** `CLAUDE.md`, `src/error.cyr:8`, `docs/port/01-PLAN.md` D2
      and `docs/port/16-serde-and-testing.md` all state that config types carry
      `#derive(Serialize)`. **None does** — `grep '#derive' src/` is 100 % `accessors`. Either
      implement it or record the drop in an ADR and fix all four documents. Note `NZone` has 32
      fields against a historically conservative 16-field derive guidance.

## 2.0.7 — Performance (all bit-identical)

Measured in the 2.0.2 sweep, verified output-preserving, deferred only for scope. Record
before/after in `bench-history.csv` per CLAUDE.md.

- [ ] **Interpolation hoist** (`src/sample.cyr`) — hoist frames/channels/data out of the tap
      loop, single interior bounds test, raw `load64` on the fast path. Measured 163 → 41–61 ns
      (2.7–4×); `fill_buffer_stereo/16` 2.708 → 1.624 ms. Zero differing bit patterns over
      55,000 positions. **The interior test must be exactly `idx-1 >= 0 && idx+2 < frames`** —
      an off-by-one here is a silent OOB read.
- [ ] **WSOLA layers 1+2** (`src/stretch.cyr`) — replace the per-frame `newprev` vec with an
      index into `input`; add a raw-pointer dot product; hoist loads out of the frame loop.
      Measured `wsola_1sec_2x` 880 → 239 ms, byte-identical over 8 configs. *(Layer 3, the
      Cauchy-Schwarz prune, is **rejected** — a hand-tuned numerical heuristic that is not
      exactly equivalent, against a "playback accuracy over speed" project rule.)*
- [ ] Non-allocating `n_instrument_find_first_zone` for `note_on` — 64-zone note_on 3.71 →
      ~1.6 µs, 1032 → 264 B/note.
- [ ] Fold filter key-tracking into `base_cutoff` at note_on; delete the per-sample `f64_pow`.
- [ ] Hermite constant hoist; `nvf_set_cutoff` literal hoist.
- [ ] Add a `FILTER_HIGHPASS` benchmark case so the non-lowpass allocation gap stops being
      invisible (`tests/nidhi.bcyr` only ever sets `FILTER_LOWPASS`).

## 2.1.0 — Structural (breaking or upstream-gated)

- [ ] **Namespace prefix sweep.** 100 top-level names skip the mandatory `n_`/`N` prefix — the
      whole `sf2_*`/`Sf2*` surface, `sfz_*` helpers, `nvf_*`, and the bare `CC_*`/`FX_*`/`LOOP_*`
      /`SM_*`/`SFZ_HDR_*`/`VEL_CURVE_*`/`FILTER_*` constant families. **Zero collisions today**
      across 253 nidhi names vs 2,216 globals in 46 `lib/*.cyr` files — but `CC_RIFF`/`CC_LIST`
      sit exactly where a codec dependency grows, shravan already parses RIFF, and Cyrius is
      **silent** on duplicate `var`. Bundle-wide churn; needs an ADR.
- [ ] **Voice-major block render.** PERF-01's headline was refuted by measurement: alone it makes
      64-voice lowpass *worse* (17.734 → 18.273 ms). The genuine 2.4× is the many-idle-slots case
      (1 voice in 64: 441.7 → 180.3 µs). Redo with real invariant hoisting, a mandatory output
      length guard, and mono coverage.
- [ ] **Streaming reader restructure.** `n_stream_reader_open` decodes the entire file, keeps
      only the format info, and throws the samples away — 85.9 ms and 14.4 MB of never-freed heap
      on a 10 s file *before the first chunk*, larger than every render-path leak 2.0.2 fixed
      combined, and the exact opposite of the API's purpose. 2.0.2 made it correct; this makes it
      cheap.
- [ ] **Per-note-on allocation** — 264 B per note_on/note_off pair, never reclaimed (~9.5 MB/hour
      at 10 notes/s). Give each voice its filter/SVF/ADSR/LFO once and re-init in place; needs a
      `filter_svf_reset` at retrigger or a voice inherits the previous note's integrator state.
- [ ] **`fill_buses_stereo`** — the only unported public function in the whole oracle surface.
      Body at `docs/port/23-mod-engine.md:763-774`.
- [ ] **`NZone_volume_db` is parsed, inherited, stored, and never read.** A `volume=-6` region
      renders at full level and the relative balance between regions is discarded. Oracle-faithful
      (rust-old never reads it either), but a field that looks wired up and is not. Implement or
      document.
- [ ] **naad dependency**: request `_filter_svf_compute_into` (a 4-output alloc-free core,
      mirroring `reverb_process_core`). Until it lands, high-pass/band-pass/notch voices leak
      ~180 MB/s at 64 voices. **Do not** inline the Cytomic/Simper core into `engine.cyr` — that
      forks DSP CLAUDE.md says must come from naad.
- [ ] Adopt the guard-page fuzz harnesses (`guard_sf2.fcyr` / `guard_sfz.fcyr`); needs a
      `cyr_mmap == -1` fallback for the agnos target.
- [ ] **WSOLA output is amplified up to ~588x** (measured: input max 0.97 → output max 588.5).
      `normalize_by_window_sum` divides by `window_sum` only where it exceeds `1e-6`, so at the
      output edges — one Hann frame, tiny taper — the division amplifies instead of normalising.
      **Inherited from the oracle, faithfully ported** (identical threshold and guard), so
      fixing it is a deliberate divergence needing an ADR. Recorded in
      `docs/port/oracle-build-identity.md` before the oracle was deleted.
- [ ] `n_stretch_ola` / `n_stretch` dedup (49 shared lines) — oracle-faithful duplication, so
      cosmetic; extract only the guard prologue and the normalize/truncate epilogue.

## Housekeeping (any release)

- [ ] `CONTRIBUTING.md` still documents the Rust workflow — MSRV 1.89, cargo-audit/cargo-deny,
      "feature-gate `naad` usage behind `#[cfg(feature = \"std\")]`". Orphaned since the port.
- [ ] `docs/port/13-hisab-lib-template.md` prescribes bare `ERR_*` in its snippet; the warning
      added in 2.0.2 covers it, but the snippet itself is still the wrong pattern to copy.
