# 0005 — Prefix the exposed names now, defer the bundle-wide sweep

**Status**: Accepted
**Date**: 2026-08-31

## Context

CLAUDE.md requires every nidhi symbol to carry an `n_` / `N` prefix, because `dist/nidhi.cyr` is
**concatenated** with `lib/{naad,goonj,shravan,sankoch,hisab}.cyr` and the stdlib into one flat
namespace. Cyrius warns on a duplicate `fn`. It is **silent on a duplicate `var`**.

**88 top-level names in `src/` do not follow it**: the whole `sf2_*` / `Sf2*` surface, the
`sfz_*` parse helpers, `nvf_*`, and the bare `CC_*` / `FX_*` / `LOOP_*` / `SM_*` / `SFZ_HDR_*` /
`VEL_CURVE_*` / `FILTER_*` constant families.

Measured this release: **0 collisions**, comparing all 88 against all **6,478** top-level names
across the 46 files in `lib/`. The risk is entirely forward-looking.

## Decision

**Prefix the two names with identifiable forward exposure. Defer the other 86.**

Renamed:

- **`CC_*` → `N_CC_*`** (13 names). These are RIFF/SoundFont chunk fourccs, and they sit in
  precisely the namespace a codec dependency grows into — **shravan already parses RIFF**. A
  future `CC_RIFF` on that side would resolve last-definition-wins with no diagnostic, and the
  failure mode is an SF2 parser that silently stops recognising chunks.
- **`ignore_i` → `n_ignore`**. Introduced in this session and shipped in `dist/nidhi.cyr`.
  `ignore` is about as collision-prone a name as exists.

## Consequences

This is a breaking change for anything referencing `CC_*`, which in practice is nothing — they
are internal SF2 parser constants with no consumer outside `src/sf2.cyr`.

The other 86 names stay as they are, and CLAUDE.md's rule stays violated for them. That is
recorded here rather than left as a silent inconsistency, because new code keeps copying the
surrounding style: `sfz_u32` and `sfz_i32`, added in 2.0.2, followed the unprefixed convention of
their neighbours.

## Why the rest is deferred rather than done

Three reasons, in order of weight.

1. **Zero measured collisions.** The benefit is preventive, and against 6,478 names in the
   current dependency set the measurement is not close. `CC_*` is the only family with a
   *named, specific* growth path into it.

2. **The naming questions are not mechanical, and getting one wrong is worse than not
   starting.** `nvf_*` already begins with `n` but not `n_`; `n_nvf_process_stereo` is
   redundant and `n_vf_process_stereo` renames the concept. `Sf2Shdr` → `NSf2Shdr` is
   consistent and ugly. Each of the ~7 families needs a decision, and in a flat-namespace
   language a partially-applied rename is a live hazard rather than a cosmetic one — naad's
   2.2.0 changelog makes exactly this point about half-prefixing an enum.

3. **It would bury the release.** 2.1.0 carries three substantive changes — zone volume applied,
   per-note filter reuse, the streaming reader restructured. An 88-symbol rename touching every
   file in `src/` and `tests/` would make that diff unreviewable, and reviewability is what
   catches the subtle one.

Prioritising by exposure is not the same as half-prefixing an enum: whole families move
together, so no family ends up split.

## What the next attempt needs

Before the remaining sweep is worth starting:

1. A decision per family, written down: `nvf_*`, `sf2_*`, `Sf2*`, `sfz_*`, and the `FX_*` /
   `LOOP_*` / `SM_*` / `SFZ_HDR_*` / `VEL_CURVE_*` / `FILTER_*` constants.
2. Re-run the collision measurement — it is one `comm` over `grep`ped top-level names and takes
   seconds; do not assume this ADR's zero still holds.
3. Land it **alone**, in its own release, verified by the full suite plus the render differential
   (`tests/golden.tcyr` and the 24,576-sample dump both survive a pure rename unchanged, so any
   movement is a real defect).
4. Note that `FILTER_*` is the family naad renamed **its** copy of in 2.2.0 specifically to
   de-collide with nidhi's. Renaming nidhi's too is safe, but the history is worth knowing.
