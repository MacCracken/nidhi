# nidhi — Roadmap

> **Forward-facing only.** What ships next, in what order, and why anything not next is not next.
> Shipped work lives in [`CHANGELOG.md`](../../CHANGELOG.md); current state is in
> [`state.md`](state.md); decisions are in [`docs/adr/`](../adr/).

Current: **2.1.3**. Every item below carries a release pin and a reason. Nothing here blocks a
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

## 2.2.0 — one item, filed upstream

- [ ] **Allocation-free four-output SVF core** — filed in **naad's own roadmap**
      (`~/Repos/naad/docs/development/roadmap.md`, "Allocation-free hot paths"), not here: an
      issue in nidhi's docs does nothing for naad. **32 B per channel per sample = 180.6 MB/s at
      64 voices**, measured; `fill_buffer_stereo_filtered_hp_8v` (1.819 ms) vs `..._8v`
      (1.528 ms) keeps the 19 % penalty visible.

      **Not blocking.** Two in-naad workarounds exist and neither is a fork:
      route those modes through `filter_biquad_process_sample` (allocation-free, covers
      HP/BP/notch, different topology so it needs an ADR), or call
      `_filter_svf_compute_lowpass` and recover the other three outputs from `k` and the
      pre/post `ic1eq` via derive accessors (bit-identical, but couples to naad's integrator
      update). Take one if naad does not land the core.

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

## Moving the cyrius pin to 6.6.6 — done; 2.1.3 went to 6.6.20

**Current pin: `cyrius = "6.6.20"`.** These notes were written against 6.6.3 and are kept as
written. The `577` respell is done. The seven `cyrius fmt --check` failures are still open,
unchanged under 6.6.20. "Nothing must change" held for 6.6.6 but not for 6.6.20: the
[2.1.3 changelog](../../CHANGELOG.md) has what that bump needed.


**Windows: one grep hit, and it is test-only.** `tests/io.tcyr:16` opens with a
bare literal — `file_open(path, 577, 420)` with the comment
`# O_WRONLY|O_CREAT|O_TRUNC, mode 0644`. That is the flag set 6.6.6 fixed on PE
(before 6.6.6 an undecoded `O_TRUNC` left the old tail of a rewritten file
behind), but nidhi has no `_TARGET_PE` / `CYRIUS_TARGET_WIN` guard anywhere in
`src/`, `programs/` or `tests/` and does not build for PE, so no nidhi behaviour
changes. Worth respelling `577` as `O_WRONLY | O_CREAT | O_TRUNC` while you are
in there — a bare 577 is the shape that hid the aarch64 `O_NOFOLLOW`/`O_LARGEFILE`
collision in a sibling.

**The struct tightening misses nidhi for the same reason it misses naad.** 27
structs (`NSamplerVoice`, `NZone`, `Sf2Phdr`, …), all used as heap layouts via
`alloc(sizeof(X))` + generated accessors. Zero `: <StructName>` annotations,
zero `StructName { … }` initializers, zero struct-to-struct copies — so the new
"copying between two different struct types is a compile error" and the
by-value-parameter deep-copy change have nothing to act on. Likewise: no `async`
fns, no `operator` fns, no `ret2`/`rethi` pair returns, no SIMD-typed returns,
no top-level `{ }` blocks, no duplicate global `var` in `src/`, no locally
defined `vec_*` (and `[deps] stdlib` already names both `assert` and `vec`, so
assert.cyr's new transitive `include "lib/vec.cyr"` cannot collide).
`lib/regression.cyr` is not vendored — the only mention is a comment at
`lib/syscalls_linux_common.cyr:208` — so the new exec deadline is irrelevant.

**Pre-existing, NOT caused by 6.6.6 — seven `cyrius fmt --check` failures.**
`src/engine.cyr:324`, `src/sample.cyr:217`, `src/zone.cyr:290`,
`tests/envelope.tcyr:28`, `tests/sample.tcyr:169`, `tests/zone.tcyr:27` and
`dist/nidhi.cyr:600` all report *"not canonically formatted … canonical
continuation indent is 2 spaces per open paren (4 also accepted)"*. **Measured
under 6.6.3, 6.6.4, 6.6.5 and 6.6.6 — identical in all four**, so the pin bump
does not introduce them, and nidhi's CI does not gate `cyrius fmt`. Clear them
with `cyrius fmt <file>` (rewrites in place) whenever, and regenerate the dist
after; not a pin blocker.

**Verify after bumping:** re-run `cyrius deps`, then `cyrius build`,
`cyrius test`, `cyrius distlib`. Measured: `cyrius build` under 6.6.3 and under
6.6.6 both exit 0 with the identical 123-line diagnostic set.
