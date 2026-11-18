# 0006 — nidhi's voice state decides voice allocation

**Status**: Accepted
**Date**: 2026-11-18

## Context

The engine keeps two pools of voices. nidhi's `NSamplerVoice` vec renders audio. naad's
`VoiceManager` picks the index for each new note (free voice first, then the steal mode). The
oracle had the same two pools (`rust-old/src/engine.rs`, `allocate_voice` and `note_off`), and
the port reproduced them faithfully. They disagree in four ways:

| Event | nidhi | naad |
|---|---|---|
| `note_off` | voice keeps sounding its release tail | slot freed at once |
| `note_off` on a retriggered note | releases **every** voice on that note | frees only the **first** slot |
| choke-group cut | voice inactive | slot still busy |
| envelope reaches IDLE, one-shot runs off its sample | voice inactive | slot still busy |

Measured against 2.2.1 (`tests/engine.tcyr`, group "nidhi's voice state decides allocation"):

- An 8-voice engine with a 500 ms release: note_on 60, note_off 60, note_on 64. naad hands 64
  the slot 60 is still releasing in, so 60's tail is cut although seven voices are idle.
- A 4-voice engine, three notes each triggered twice and released once, then rendered until
  silent. naad still reports 3 slots busy, so a 3-note chord sounds on 2 voices. Polyphony stays
  collapsed until `all_notes_off`. Repeated notes are ordinary playing (a piano trill, a drum
  roll), so this is not an edge case.

## Decision

**nidhi's per-voice `active` flag is the truth.** Just before naad allocates, `note_on` copies
it into naad's pool (`n_engine_sync_voice_mgr`). naad still picks the index, with the same free
search and the same steal modes, but always over what is really sounding.

- naad's `note_off` and `all_notes_off` are still called. They no longer matter for allocation,
  and keeping them leaves naad's state as close to nidhi's as it was between notes.
- Ages are not synced. The engine never ticks naad's ages, so Oldest steals the highest active
  index, as before, which is the oracle's tie-break.

## Consequences

- **Positive**: release tails survive while free voices exist, and a retriggered note no longer
  leaks a voice. Choke cuts and naturally ended voices free their slots.
- **Negative**: the output differs from the oracle whenever the pools would have disagreed. That
  is the point, but it is a divergence. One pass over the voices per `note_on`, with no allocation.
- **Neutral**: a voice's two SVFs are built lazily on its first filtered note. Notes now spread
  over more voices, so more voices reach that one-time build sooner. `tests/engine.tcyr`'s
  note_on allocation check warms every voice before it measures, and steady state is still 0 B.

## Alternatives considered

- **Drop naad's VoiceManager and allocate in nidhi.** This was the oracle's no_std path. It is
  cleaner, but it re-implements three steal modes naad already owns ("use naad fully"), and it
  changes steal tie-breaks.
- **Call naad's `note_off` once per matching voice.** That fixes the retrigger leak only: tails
  would still be cut and chokes would still leak.
