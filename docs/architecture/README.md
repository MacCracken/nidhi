# Architecture notes

Non-obvious constraints, quirks, and invariants that a reader cannot derive from the code alone. Numbered chronologically — never renumber.

Not decisions (those live in [`../adr/`](../adr/)) and not guides (those live in [`../guides/`](../guides/)). An item here describes *how the world is*, not *what we chose* or *how to do something*.

## Items

| Note | Why it is here |
|---|---|
| [001 — The allocator never frees, so allocation on the render path is fatal, not slow](001-the-allocator-never-frees.md) | Explains why per-sample and per-note allocation is treated as a defect class rather than an inefficiency, and why `alloc()` returning 0 is a memory-safety problem. |
| [002 — Everything shares one flat namespace, and duplicate `var` is silent](002-one-flat-namespace.md) | Explains the `n_`/`N` prefix rule, why tests include modules the bundle would supply, and why a constant collision has no symptom until the values disagree. |

Add a numbered entry (`NNN-kebab-case-title.md`) the next time the code has a non-obvious
invariant a reader cannot derive. Not decisions — those are ADRs.
