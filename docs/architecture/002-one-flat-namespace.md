# 002 — Everything shares one flat namespace, and duplicate `var` is silent

`dist/nidhi.cyr` is a **plain concatenation** of `cyrius.cyml [lib].modules` in order. A consumer
then concatenates *that* with `lib/naad.cyr`, `lib/goonj.cyr`, `lib/shravan.cyr`,
`lib/sankoch.cyr`, `lib/hisab.cyr` and the stdlib. There is no module system, no namespacing, and
no visibility: every top-level `fn`, `var` and `struct` in every one of those files occupies one
shared global scope.

Two consequences a reader cannot derive from `src/`.

## Duplicate `fn` warns. Duplicate `var` does not.

Cyrius emits `duplicate fn 'x' (last definition wins)` for functions. For variables it emits
**nothing at all** — the later definition silently wins.

That asymmetry is the whole hazard. A constant collision produces no diagnostic, no test failure,
and no symptom *until the two sides disagree on a value*. nidhi shipped exactly this: `ERR_NONE`
was defined as `0` by nidhi, by `lib/goonj.cyr`, and by shravan's `enum ShravanErr`
simultaneously. Nothing misbehaved, because all three agreed — which is precisely what made it
dangerous. Renamed to `NIDHI_ERR_*` in 2.0.2.

naad hit the same trap from the other side and renamed its `FILTER_*` / `ERR_*` / `VOICE_*`
families in 2.2.0 specifically to de-collide with nidhi's.

## Hence the `n_` / `N` prefix rule

Every nidhi symbol carries `n_` (functions, constants) or `N` (types). It is not stylistic. As of
2.1.0, 86 top-level names still skip it — measured at **0 collisions** against all 6,478
top-level names in `lib/`, so the exposure is forward-looking rather than live. The two with a
named growth path (`CC_*`, sitting where a RIFF-parsing codec dependency would collide, and
`ignore_i`) were prefixed in 2.1.0; the rest is
[ADR 0005](../adr/0005-namespace-prefix-scope.md).

## Practical rules this implies

- **No `include` between `src/*.cyr` files.** Concatenation order *is* the dependency graph, so a
  module may reference symbols from any module above it in `[lib].modules` and none below.
- **Tests must include what they use.** A `.tcyr` compiles a different unit from the bundle, so
  `error.cyr` and `f64_util.cyr` have to be included explicitly even though the bundle would have
  supplied them.
- **Re-measure before assuming the namespace is clear.** It is one `comm` over `grep`ped
  top-level names and takes seconds. A dependency bump can introduce a collision with no
  diagnostic whatsoever.
