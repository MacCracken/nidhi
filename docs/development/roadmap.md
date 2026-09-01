# nidhi — Roadmap

> **Forward-facing only.** What ships next, in what order, and why anything not next is not next.
> Shipped work lives in [`CHANGELOG.md`](../../CHANGELOG.md); current state is in
> [`state.md`](state.md); decisions are in [`docs/adr/`](../adr/).

Current: **2.1.0**. Every item below carries a release pin and a reason. Nothing here blocks a
release — 2.1.0 is complete and green.

---

## 2.1.1 — the work nidhi can do alone

Ordered by measured harm. All of it is nidhi-side; nothing waits on a dependency.

- [ ] **`fill_buses_stereo` — implement real bus routing, not the oracle's stub.**
      The last unported public function from the Rust surface, but porting it verbatim is worse
      than not porting it: the oracle's own implementation accumulates the whole mix into
      `buses[0]` and ignores `output_bus` entirely ("per-voice bus routing is planned for a
      future release"). `NZone_output_bus` is already parsed from SFZ `output=`, inherited, and
      stored — **and read by nothing**, which is exactly the shape `volume_db` had before 2.1.0
      ([ADR 0004](../adr/0004-apply-zone-volume-db.md)). Give the voice an `output_bus` at
      note-on and accumulate per bus. Additive API; nothing in-tree consumes it yet.
- [ ] **WSOLA amplifies output up to ~588x.** `normalize_by_window_sum` divides by `window_sum`
      only where it exceeds `1e-6`, so at the output edges — one Hann frame, tiny taper — the
      division amplifies instead of normalising. Measured: input max 0.97 -> output max 588.5, on
      every fixture tried. **Faithfully inherited from the oracle** (identical threshold and
      guard), so fixing it is a deliberate divergence: needs an ADR, a listening check, and new
      golden vectors, since `tests/golden.tcyr` currently pins the *current* behaviour by
      sampling the interior deliberately.
- [ ] **Guard-page fuzz harnesses** (`guard_sf2.fcyr` / `guard_sfz.fcyr`). Harness-only, no `src/`
      change; the shipped parsers already pass. Needs a `cyr_mmap == -1` fallback for the agnos
      target.
- [ ] **`n_stretch` / `n_stretch_ola` dedup** — 49 shared lines. Oracle-faithful duplication with
      no wrong behaviour, so this is tidying, not repair. Extract only the guard prologue and the
      normalize/truncate epilogue; a single mode-flagged helper would make the OLA path carry
      `prev`/`tolerance` bookkeeping it never uses.

## 2.2.0 — gated on naad

Both filed upstream. nidhi cannot fix either without forking DSP that CLAUDE.md says must come
from naad.

- [ ] **Allocation-free SVF core for high-pass / band-pass / notch** —
      [`issues/2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md`](issues/2026-08-31-naad-no-alloc-free-svf-core-for-non-lowpass.md).
      **32 B per channel per sample = 180.6 MB/s at 64 voices**, measured. Low-pass already has
      the escape hatch and nidhi took it in 2.0.7; the other three modes have none.
      `fill_buffer_stereo_filtered_hp_8v` (1.819 ms) vs `..._8v` (1.528 ms) keeps the **19 %**
      penalty visible. *On arrival:* route all four modes through it, delete the mode branch in
      `nvf_process_stereo`, extend the zero-allocation assertion to all four.
- [ ] **`Adsr` / `Lfo` re-arm APIs** —
      [`issues/2026-08-31-naad-adsr-and-lfo-cannot-be-re-armed.md`](issues/2026-08-31-naad-adsr-and-lfo-cannot-be-re-armed.md).
      **72 B per note-on**, never reclaimed. 2.1.0 removed the filter half (264 -> 72 B) *because*
      `filter_svf_set_params` and `filter_svf_reset` exist; `Adsr` and `Lfo` have no equivalent.
      *On arrival:* give each voice its envelope and LFOs once in `n_voice_new`, re-arm in
      `note_on`, take per-note allocation to zero.
- [ ] **Genuinely incremental streaming.** 2.1.0 made `open()` decode once instead of twice, but
      it still holds the whole decoded file. shravan's decoder is buffer-then-decode-once — the
      defect behind the 2.0.2 4 KB truncation bug — so real streaming needs an incremental
      decoder there. File against shravan when it becomes worth doing.

## Deferred — with the reason, so it is not re-opened blind

Each of these was examined and consciously not done. The reason is the point.

| Item | Pin | Why it is deferred |
|---|---|---|
| **The 86 remaining unprefixed names** | 2.2.0, alone | [ADR 0005](../adr/0005-namespace-prefix-scope.md). **0 collisions measured** across all 6,478 top-level names in `lib/`, so the benefit is purely preventive. The naming questions are not mechanical (`nvf_*` already starts with `n` but not `n_`; `Sf2Shdr` -> `NSf2Shdr` is consistent and ugly), and a partially-applied rename in a flat-namespace language is a live hazard rather than a cosmetic one. The two with a *named* forward path (`CC_*`, `ignore_i`) were renamed in 2.1.0. **Land the rest in its own release**; the ADR lists the per-family decisions needed first. |
| **Voice-major block render** | 2.2.0 | The headline claim was **refuted by measurement**: alone it makes 64-voice low-pass *worse* (17.734 -> 18.273 ms). The genuine 2.4x is only the many-idle-slots case (1 voice in 64: 441.7 -> 180.3 us). 2.0.7 took most of the per-voice win by other means, so what remains is a narrower prize than it looked. Redo only with real invariant hoisting, a mandatory output-length guard, and mono coverage. |
| **Serde** | unpinned — needs a consumer | [ADR 0003](../adr/0003-serde-is-not-ported.md). Never implemented despite four documents claiming it was; those are corrected. Implementing it is a **feature, not a repair** — `NZone` has 32 fields against a 16-field derive guidance, and with no traits there is no `Deserialize` to derive, so every type needs a hand-written `from_json`. Pin it when something actually needs to persist a config. |
| **NaN clamping to a bound** | no change planned | [ADR 0002](../adr/0002-nan-clamps-to-the-bound.md). A NaN parameter yields a finite value at a clamp bound where Rust propagated NaN. **Kept deliberately** — a NaN reaching the SVF latches the voice for its whole life. The "ignore non-finite input entirely" alternative is arguably better still and is recorded there; revisit only if the hard-left-pan-on-NaN surprise is ever reported. |
| **PERF-03 layer 3** (Cauchy-Schwarz prune) | **rejected**, not deferred | A numerical heuristic with a hand-tuned margin, not an exact transformation — it trades audio accuracy for speed against an explicit project rule. The two exact WSOLA layers delivered 4.02x without it. |

## Housekeeping — any release

- [ ] `docs/port/12-vidya-port-template.md` is a template for porting *other* projects and still
      describes a live `rust-old/`. Harmless here, but it is the file a future port would copy.
- [ ] `docs/port/13-hisab-lib-template.md` shows bare `ERR_*` in its snippet. A warning was added
      in 2.0.2, but the snippet itself is still the wrong pattern to copy.
