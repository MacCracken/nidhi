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
| **2.0.5** (2026-08-31) | Coverage backfill — every named oracle test now has a counterpart; 441 → 518 assertions |
| **2.0.6** (2026-08-31) | Latent-hazard closeout; ADRs 0002 (NaN clamps) and 0003 (serde not ported) |

---

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
