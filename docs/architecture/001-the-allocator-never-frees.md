# 001 — The allocator never frees, so allocation on the render path is fatal, not slow

`lib/alloc.cyr` is a **bump allocator**. `alloc(n)` hands out the next `n` bytes and advances a
pointer; the free path (`_alloc_free_noop`) does nothing. There is no reclamation of any kind
short of `alloc_reset()`, which invalidates every live pointer at once and no audio code can call.

This single fact explains more decisions in this codebase than any other, and it is not derivable
from reading `src/`.

## What follows from it

**Allocation on a per-sample or per-note path is unbounded growth for the life of the process.**
Not churn a good allocator would absorb — growth. Three measured examples, all fixed:

| | rate | fixed in |
|---|---|---|
| Render scratch (3 × `alloc(16)` per frame) | 2,116,800 B/s at 44.1 kHz | 2.0.2 |
| naad's allocating one-shot SVF | 180.6 MB/s at 64 voices | 2.0.7 (low-pass only) |
| Reverb's allocating wrapper | 1,411,200 B/s | 2.0.2 |
| A filter per `note_on` | 264 B/note | 2.1.0 (→ 72 B) |

**The endgame is memory unsafety, not an out-of-memory error.** `alloc()` returns **0** on
failure, and Cyrius has no null-pointer trap. A render path that does not check it goes on to
`store64(0, ...)` — a segfault inside the audio callback. That is why `n_engine_new` checks all
four of its scratch slots, and why `tests/engine.tcyr` asserts an `alloc_used()` delta of exactly
zero across a rendered block rather than merely a small one.

**`vec` death is an unconditional process exit.** `vec_push` on a failed grow, and any
out-of-range `vec_get`/`vec_set`, call `_vec_die()` → `syscall(60, 1)`. There is no error return
and nothing to catch: a bad index in nidhi takes down the host (dhvani, and thereby shruti) with
it. This is why the SF2 allocation bomb (2.0.2) and the unbounded SFZ `group=` index (2.0.2) were
treated as security defects rather than robustness nits — a ~7 KB file could kill the process.

**Peak memory is total memory.** Freeing a large intermediate does not help, so the fix is always
to not allocate it: the WSOLA `prev` frame became an index into `input` rather than a copy
(2.0.7, ~2.8 MB per stretch), and the streaming reader stopped decoding every file twice (2.1.0).

## What this does NOT mean

Allocation at construction time is fine and normal. `n_engine_new` allocating per-engine scratch,
`n_voice_new` allocating a filter per voice, and parsers allocating their result vectors are all
correct — they happen once, bounded by the caller's own structure. The rule is specifically about
paths whose call count is driven by *audio time* or *user activity*.
