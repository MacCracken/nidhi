# 0012 — note_on round-robins, and SFZ sequences are real

**Status**: Accepted
**Date**: 2026-11-18

## Context

nidhi had round-robin selection and never used it. Three things were wrong, all inherited from
the oracle:

1. **`note_on` played the first matching zone.** The oracle's `note_on` took
   `instrument.find_zones(note, velocity)[0]` (`rust-old/src/engine.rs:543`), so its own
   `Instrument::find_zone_rr`, which cycles the zones of a group, was reachable from the API only.
   A zone group set with `n_zone_with_group` did nothing in the engine.
2. **SFZ `seq_position` wrote the round-robin group** (`rust-old/src/sfz.rs:275`). A standard
   round-robin set gives each region a different position:

   ```
   <group> seq_length=3 key=60
   <region> sample=a1.wav seq_position=1
   <region> sample=a2.wav seq_position=2
   <region> sample=a3.wav seq_position=3
   ```

   That put the three regions in groups 1, 2 and 3, and `find_zone_rr` cycles only the zones that
   share the first match's group. So even with (1) fixed, every note would play `a1.wav`.
3. **`seq_length` was not parsed at all.**

The 2.2.2 roadmap said fixing this was "one call in note_on". That was wrong because of (2).

## Decision

- **`note_on` selects through `n_instrument_find_zone_rr`.**
- **Zones carry SFZ's sequence**: `seq_length` and `seq_position`, both 1 by default, set with
  `n_zone_with_seq`. A sequenced zone sounds on the note-ons where its counter
  `% seq_length == seq_position - 1`.
  - Each zone has its own counter, kept by the instrument and parallel to its zones. It advances
    on every note-on the zone matches, sounding or not, as in SFZ players.
  - The regions of one set advance in lockstep, and sets on different keys advance
    independently.
  - A `seq_position` past `seq_length` never sounds, as in SFZ.
- **`find_zone_rr` filters by sequence step first**, then applies the oracle's group round-robin
  to what is left. Every zone the oracle could build has `seq_length = 1`, so for those it is the
  oracle's function exactly.
- **SFZ**: `seq_length` and `seq_position` parse into the region (u32) and inherit through
  `<group>` and `<global>`, and `to_zones` writes them to the zone. `seq_position` no longer writes
  the group. `group=` still writes the round-robin group, as in the oracle.

## Consequences

- **Positive**:
  - SFZ round-robin sets cycle.
  - API groups cycle in the engine.
  - A single-zone or ungrouped, unsequenced instrument plays exactly as before, and the release
    differential is unchanged by this decision.
  - Selection allocates nothing.
- **Negative**:
  - Zones that match the same note and share an SFZ `group=` number now alternate. Before, the
    first always played. In SFZ, `group=` is an exclusive class for `off_by` (choke), and
    co-matching regions are layers that should all sound. nidhi implements neither: it plays one
    zone per note, and the oracle mapped `group=` to round-robin. So both the old and the new
    behaviour differ from SFZ's here; this ADR keeps the oracle's mapping rather than widen the
    divergence.
  - `NZone` grows from 32 to 34 fields and `NSfzRegion` from 37 to 39. Both are additive:
    `sizeof` follows the struct, and every existing accessor keeps its meaning.
- **Neutral**: `find_zone_rr` now also advances sequence counters. It already advanced group
  counters, so it was never a pure query.

## Alternatives considered

- **Only call `find_zone_rr` from note_on.** That cycles API groups and leaves every real SFZ
  round-robin playing its first region, because of (2).
- **Map each SFZ sequence set to one shared group, ordered by position.** That reuses the group
  machinery, but a group's counter is shared by every key that uses the group, so playing C then
  D would advance one counter, and the zones would have to be reordered by position. Per-zone
  counters are what SFZ players keep, and they need no reordering.
- **Re-map SFZ `group=` to choke and add `off_by`.** That is the correct SFZ semantics, and it is
  a larger change (exclusive classes and `off_by` lists). It is on the roadmap.
