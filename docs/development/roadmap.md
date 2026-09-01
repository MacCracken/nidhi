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
| **2.0.7** (2026-08-31) | Performance, all bit-identical — WSOLA 4.02×, interpolation 2.8×, 64-voice 1.64× |
| **2.1.0** (2026-08-31) | Zone `volume` applied (ADR 0004); per-note filter reuse; streaming reader decodes once; `CC_*` → `N_CC_*` |

---

## 2.1.1 / 2.2.0 — what is left

Ordered by measured harm. Nothing here is blocking a release.

### Upstream-gated on naad

- [ ] **High-pass / band-pass / notch voices allocate ~180 MB/s at 64 voices.** naad ships an
      allocation-free SVF core for **low-pass only**. `fill_buffer_stereo_filtered_hp_8v` (added
      2.0.7) makes the cost visible: 1.82 ms vs 1.53 ms for the identical low-pass workload, a
      19 % penalty. Needs `_filter_svf_compute_into(self, input, out4)` from naad, mirroring
      `reverb_process_core`. **Do not inline the Cytomic/Simper core into `engine.cyr`** — that
      forks DSP CLAUDE.md says must come from naad.
- [ ] **The rest of the per-note allocation (72 B/note).** 2.1.0 removed the filter half; the
      envelope and LFOs remain because naad has no `envelope_adsr_reset` and no LFO frequency
      setter, so an `Adsr` cannot be re-armed for a different zone's times.

### Deferred with reasons recorded

- [ ] **The other 86 unprefixed names** — [ADR 0005](../adr/0005-namespace-prefix-scope.md).
      0 collisions measured against 6,478 lib names. The ADR lists the per-family naming
      decisions the next attempt needs, and says to land it alone.
- [ ] **Voice-major block render.** PERF-01's headline was refuted by measurement: alone it makes
      64-voice low-pass *worse* (17.734 → 18.273 ms). The genuine 2.4× is the many-idle-slots
      case (1 voice in 64: 441.7 → 180.3 µs). Redo with real invariant hoisting, a mandatory
      output-length guard, and mono coverage. Note 2.0.7 already took most of the per-voice win
      by other means.
- [ ] **`fill_buses_stereo`.** The only unported public function in the oracle's surface — but
      the oracle's own implementation is a stub: it accumulates the full mix into `buses[0]` and
      ignores `output_bus` entirely ("per-voice bus routing is planned for a future release").
      So porting it verbatim ships a function that does not do what its name says. Real routing
      is the honest version and is a feature: give each voice an `output_bus` from its zone and
      accumulate per bus. `NZone_output_bus` is already parsed, inherited and stored — the same
      shape as `volume_db` before 2.1.0.
- [ ] **Genuinely incremental streaming.** 2.1.0 made `open()` decode once instead of twice, but
      it still holds the whole decoded file. Needs an incremental decoder from shravan.
- [ ] **WSOLA amplifies output up to ~588×** where `window_sum` falls under the `1e-6` normalise
      threshold. Faithfully inherited from the oracle (identical threshold and guard), so fixing
      it is a deliberate divergence needing its own ADR and a listening check.
- [ ] **Serde**, if it is ever wanted — [ADR 0003](../adr/0003-serde-is-not-ported.md). A
      feature, not a repair.
- [ ] Guard-page fuzz harnesses (`guard_sf2.fcyr` / `guard_sfz.fcyr`); needs a `cyr_mmap == -1`
      fallback for the agnos target.
- [ ] `n_stretch` / `n_stretch_ola` dedup — 49 shared lines, oracle-faithful duplication, no
      wrong behaviour. Extract only the guard prologue and the normalize/truncate epilogue.

## Housekeeping (any release)

- [ ] `CONTRIBUTING.md` still documents the Rust workflow — MSRV 1.89, cargo-audit/cargo-deny,
      "feature-gate `naad` usage behind `#[cfg(feature = \"std\")]`". Orphaned since the port.
- [ ] `docs/port/13-hisab-lib-template.md` prescribes bare `ERR_*` in its snippet; the warning
      added in 2.0.2 covers it, but the snippet itself is still the wrong pattern to copy.
