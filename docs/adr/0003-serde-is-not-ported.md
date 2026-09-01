# 0003 — Serde serialization is not ported

**Status**: Accepted
**Date**: 2026-08-31

## Context

The Rust crate derived `Serialize + Deserialize` on essentially every public type — `Zone`,
`AdsrConfig`, `LoopMode`, `VelocityCurve`, `FilterMode`, `EnvState`, `Sample`, `SampleBank`,
`Instrument`, `SampleRecorder`, `TimeStretcher`, `StretchMode`, `EffectChain`, `EffectSlot`,
`EffectType`, `SfzFile`, `SfzRegion`, `Sf2Preset` — and `lib.rs` had a round-trip test asserting
JSON serialize→deserialize for all of them.

**Four documents in this repo state that the port carries this forward. None of it exists.**

- `CLAUDE.md`: "config types use `#derive(Serialize)` (bayan JSON) for the serde-roundtrip
  requirement"
- `src/error.cyr`: "Config/data types that need a faithful serde round-trip use
  `#derive(Serialize)` instead (see zone.cyr etc.)" — `zone.cyr` does not have it
- `docs/port/01-PLAN.md` decision D2
- `docs/port/16-serde-and-testing.md`: records "every type Serialize+Deserialize" as a
  first-party requirement

`grep '#derive' src/` is **100 % `accessors`**. The 2.0.3 port-coverage audit found this across
five of its six module groups independently; it was the single most-repeated finding.

The skip was taken silently during the port. `docs/port/20-mod-core.md` offered the escape hatch
("reimplement by hand … else skip") and nobody wrote down which branch was taken.

## Decision

**Serde is not ported, and the four documents are corrected to say so.**

## Consequences

Nothing in the repo regresses: no `src/` code, no test, and no consumer calls a serialization
path today, because none exists to call.

What is genuinely lost is the Rust round-trip test's *second* job. It was not only checking
serde — it built a `Zone` through 23 chained builders as its fixture, and
`docs/port/20-mod-core.md` called that "the single best fixture for Zone field parity". That
fixture is recovered independently in `tests/zone.tcyr` (see 2.0.5), so the coverage survives
even though the mechanism does not.

A consumer that needs to persist a nidhi config must serialize it field by field through the
`#derive(accessors)` getters. That is more work than a derive, and it is honest about what the
library provides.

Two facts make revisiting this non-trivial rather than a small chore:

- `docs/port/16-serde-and-testing.md` records a **16-field** guidance for
  `#derive(Serialize)`/`accessors` structs (the hard cap was raised 32→256 at cycc 6.0.47, but
  the guidance stayed conservative because nidhi's structs are wide). `NZone` has **32 fields**.
- Cyrius has no traits, so there is no `Deserialize` counterpart to derive — a from-JSON path
  would be hand-written per type against `bayan`.

## Consequences if this is reversed later

Implementing it is a feature, not a repair, and belongs in a minor release. It would need: a
decision on `NZone`'s field count against the derive guidance, hand-written `from_json` for
every type, and a round-trip test per type. Tracked in `docs/development/roadmap.md` under
2.1.0, not as a defect.

## Alternatives rejected

- **Implement `#derive(Serialize)` now to make the documents true.** The documents were wrong;
  the code was not. Adding an unrequested, untested, unconsumed serialization surface to a patch
  release to retro-justify four sentences is the wrong direction — and the `NZone` field-count
  question would have to be answered under time pressure.
- **Leave the documents alone.** They actively mislead. `src/error.cyr` points a reader at
  `zone.cyr` "for an example", and there is nothing there.
