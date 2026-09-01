# 0004 — Zone `volume_db` is applied to the rendered voice

**Status**: Accepted
**Date**: 2026-08-31

## Context

`NZone.volume_db` was parsed from the SFZ `volume=` opcode (`src/sfz.cyr`), inherited through
`<group>` and `<global>`, clamped, stored on the zone, exposed through a public accessor — and
**read by nothing**. Voice amplitude was `velocity_curve × envelope × pressure`, with no dB term
anywhere in the render path.

Two consequences, and the second is the serious one:

- `<region> volume=-6` rendered at **full level**.
- The *relative* balance a multi-region instrument encodes was silently discarded. A layered
  patch that sets `volume=-3` on its soft layer and `volume=0` on its hard layer played both at
  the same level — which is not "slightly wrong", it is the wrong instrument.

**The oracle had the same gap.** `rust-old/src/engine.rs` never read `zone.volume_db` either, so
this is not a port defect and reproducing it was, strictly, parity-faithful.

The 2.0.3 port-coverage audit flagged it as "a field that looks wired up and is not" — the worst
kind of dead code, because every layer above it behaves as though the feature exists.

## Decision

Apply it. Voice amplitude is now
`velocity_curve × 10^(volume_db / 20) × envelope × pressure`, with the dB→linear conversion done
once at note-on via naad's `naad_db_to_amplitude`.

Folded in at note-on rather than per sample: a zone's volume cannot change for the life of a
voice, so this costs one `f64_pow` per note-on and nothing per frame.

## Consequences

**This changes rendered audio** for any zone with a non-zero `volume`, which in practice means
most real SFZ instruments. It is a deliberate divergence from the oracle.

It is also the change most likely to be *noticed* as an improvement: `volume=` is a core SFZ
opcode, and a sampler that ignores it cannot play a correctly-authored instrument. Anyone who had
compensated for the old behaviour by pre-scaling their samples will need to undo that.

The guard is `if (vol_db != 0)`, so a zone that never sets `volume` takes no new work and renders
bit-identically to 2.0.7. Only zones that opted in change.

Pinned in `tests/engine.tcyr` under "zone volume_db reaches the output", which asserts the
*ratio* of peak levels rather than absolute values — `-6 dB → ×0.501`, `+6 dB → ×1.995` — so the
test does not depend on the source material.

## Alternatives rejected

- **Keep oracle parity and leave it unimplemented.** Defensible on the letter of the parity bar,
  indefensible on the purpose of the library. CLAUDE.md's bar is feature-set parity with cleaner
  routes allowed, and nidhi's stated job is SFZ playback.
- **Document it as unimplemented and remove the field.** Removing it would break the SFZ parser's
  inheritance chain, which genuinely computes the value, and would throw away work already done
  correctly everywhere except the last step.
- **Apply it per sample in the render loop.** Same result, measurably worse: an `f64_pow` per
  sample per voice for a value that is constant across the voice's life. This is the same mistake
  2.0.7 removed for filter key-tracking.
