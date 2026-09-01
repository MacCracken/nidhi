# 0001 — Loop crossfade reads its fade-in source backward from `loop_start`

**Status**: Accepted
**Date**: 2026-08-31

## Context

nidhi's loop crossfade is a direct transcription of `rust-old/src/engine.rs:813-829`:

```rust
let xfade_pos = zone.loop_start as f64 + (xfade_f - dist_to_end);
```

`dist_to_end` counts down from `xfade` to `0` as the playhead approaches `loop_end`, so this
walks **forward** from `loop_start`. At the wrap itself (`dist_to_end == 0`) it reads
`loop_start + xfade` — and the very next frame, now wrapped, plays `loop_start`.

The seam therefore ends on a jump of `xfade` samples *backwards* through the material. The
crossfade attenuates the click; it never closes it. That is a coherent technique only if the
loop also wraps to `loop_start + xfade`, and it does not — the wrap target is plain
`loop_start`. The port inherited a formula from one convention and a wrap point from the other.

Two related defects sat alongside it:

- `crossfade_length` was never validated against the loop length, so a crossfade longer than the
  loop was live from frame 0 and the loop's own material was never heard at full level.
- There was **no test and no benchmark** for crossfade anywhere in the repo
  (`grep -n crossfade tests/*.tcyr` returned nothing), so none of this was measurable.

CLAUDE.md states "playback accuracy over speed; sample-accurate loop points and crossfades",
which makes this a defect against a stated project requirement rather than a cosmetic nicety.

## Decision

Read the fade-in source **backward** from `loop_start`:

```
xfade_pos = loop_start - dist
```

The material that must follow `loop_end` is the material that *precedes* `loop_start`, because
that is what naturally leads into `loop_start`. At the wrap (`dist == 0`) this reads exactly
`loop_start`, which is where the post-wrap playhead sits. The seam closes, and the loop's period
is unchanged — so its pitch is unchanged.

Two clamps, both load-bearing:

- **against the loop length** — `xfade = min(crossfade_length, loop_end - loop_start)`.
- **against `loop_start`** — there is no material before frame 0, and reads there return
  zero-padding, so an unclamped backward fade blends in **silence** and attenuates the loop by
  up to 50 %. The naive `loop_start - dist` alone is wrong for this reason.

When `loop_start < xfade` there is genuinely no pre-roll, and **no** crossfade can close a seam
whose endpoints differ; only moving the wrap point could, which would change the loop's period
and therefore its pitch. That case keeps the legacy forward blend. Closure where it is
achievable, never worse than 2.0.2 where it is not.

## Consequences

**This changes rendered audio** for any zone with `crossfade_length > 0`, a forward or sustain
loop, and `loop_start >= crossfade_length`. It is a deliberate divergence from the oracle.

Measured on a 400-frame linear ramp (a signal where a correct crossfade is *exactly* continuous),
as the largest frame-to-frame discontinuity across the seam, ×1e6:

| case | 2.0.2 | 2.0.3 |
|---|---:|---:|
| `xfade=20, loop 100..200` | 22,999 | **4,000** |
| `xfade=0` (control, no crossfade) | 99,000 | 99,000 |
| `xfade=20, loop 0..200` (no pre-roll) | 28,000 | 28,000 |
| `xfade=500` (longer than the 100-frame loop) | 19,602 | **1,000** |

The control is unchanged, the no-pre-roll case is unchanged, and the two cases the fix targets
improve by 5.7× and 19.6×. Pinned in `tests/engine.tcyr` under "loop crossfade closes the seam".

Anything comparing against stored 2.0.2 renders of crossfaded loops will differ. Nothing in the
repo did, because nothing tested crossfade at all.

## Alternatives rejected

- **Keep oracle parity.** The bar in CLAUDE.md is feature-set parity with cleaner routes allowed,
  and "sample-accurate crossfades" is an explicit requirement. Reproducing a defect that fails
  the project's own stated goal is not parity worth having.
- **Convention B — blend forward and move the wrap to `loop_start + xfade`.** Also coherent, and
  it works with no pre-roll. Rejected because it shortens the loop, changing its period and so
  its pitch — a worse trade for a sampler than requiring pre-roll material.
- **Clamp `xfade` to `loop_start` and disable the fade when there is no pre-roll.** Measurably
  worse than shipping nothing: the ramp loop at `loop_start=0` went from a 0.028 seam to 0.199.
