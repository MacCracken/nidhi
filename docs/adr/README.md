# Architecture Decision Records

Decisions about nidhi — what we chose, the context, and the consequences we accept. Use these when a future reader would reasonably ask *"why did we do it this way?"*

## Conventions

- **Filename**: `NNNN-kebab-case-title.md`, zero-padded to four digits. Never renumber.
- **One decision per ADR.** If a decision supersedes a prior one, add a new ADR and set the old one's status to `Superseded by NNNN`.
- **Status lifecycle**: `Proposed` → `Accepted` → (optionally) `Superseded` or `Deprecated`.
- Use [`template.md`](template.md) as the starting point.

## ADR vs. architecture note vs. guide

| Kind | Lives in | Answers |
|---|---|---|
| ADR | `docs/adr/` | *Why did we choose X over Y?* |
| Architecture note | `docs/architecture/` | *What non-obvious constraint is true about the code?* |
| Guide | `docs/guides/` | *How do I do X?* |

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-loop-crossfade-seam.md) | Loop crossfade reads its fade-in source backward from `loop_start` | Accepted |
| [0002](0002-nan-clamps-to-the-bound.md) | A NaN parameter clamps to a bound instead of propagating | Accepted |
| [0003](0003-serde-is-not-ported.md) | Serde serialization is not ported | Accepted |
| [0004](0004-apply-zone-volume-db.md) | Zone `volume_db` is applied to the rendered voice | Accepted |
| [0005](0005-namespace-prefix-scope.md) | Prefix the exposed names now, defer the bundle-wide sweep | Accepted |
| [0006](0006-nidhi-voice-state-decides-allocation.md) | nidhi's voice state decides voice allocation | Accepted |
| [0007](0007-filter-velocity-tracking-lasts-the-note.md) | Filter velocity tracking lasts the whole note | Accepted |
| [0008](0008-released-sustain-loop-skips-the-crossfade.md) | A released LoopSustain voice skips the loop crossfade | Accepted |
| [0009](0009-playback-speed-includes-sample-rate.md) | Playback speed includes the sample's own rate | Accepted |
| [0010](0010-multichannel-loads-front-pair.md) | A file with more than two channels loads its front pair | Accepted |
| [0011](0011-effect-slots-get-the-parameters-meant.md) | The reverb and chorus slots get the parameters the oracle meant | Accepted |
| [0012](0012-round-robin-plays.md) | note_on round-robins, and SFZ sequences are real | Accepted |
