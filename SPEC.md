# SPEC

Engine specification for `formalmethods`. Ported 1:1 from the TypeScript
engine in `../formalmethods` (`src/lib/formal/`).

## LTL grammar

Loosest to tightest binding; ASCII surface syntax only.

```
iff      := implies ("<->" implies)*
implies  := or ("->" implies)?           // right-associative
or       := and ("||" and)*
and      := until ("&&" until)*
until    := unary (("U" | "W" | "R") unary)*
unary    := ("!" | "X" | "G" | "F") unary | atom
atom     := "(" iff ")" | "true" | "false" | IDENT
IDENT    := [A-Za-z_][A-Za-z0-9_]*
```

- Single `&` / `|` are accepted and mean `&&` / `||`.
- `X`, `G`, `F`, `U`, `W`, `R`, `true`, `false` are reserved — they cannot
  be atom names.
- `print_ltl` emits the same syntax; parse ∘ print round-trips.

## Formula JSON AST

`Formula` serializes as the TS engine's tagged union (`serde`), so ASTs can
cross process boundaries:

```json
{"kind": "atom", "name": "coin"}
{"kind": "not", "arg": <formula>}
{"kind": "until", "left": <formula>, "right": <formula>}
```

Kinds: `true`, `false`, `atom`, `not`, `and`, `or`, `implies`, `iff`,
`next`, `globally`, `finally`, `until`, `weak`, `release`. `arg` for the
unary kinds, `left`/`right` for binary, `name` for `atom`.

## Semantics

### Finite traces — LTLf (`satisfies_trace`, `eval_at`)

Over `Trace = Vec<Set<atom>>` of length `n`, at position `i`:

| Formula | Holds iff |
| --- | --- |
| `X p` | `i+1 < n` and `p` at `i+1` (false at the last position) |
| `F p` | some `j ∈ [i, n)` has `p` |
| `G p` | every `j ∈ [i, n)` has `p` |
| `p U q` | some `j` has `q`, and `p` holds on all of `[i, j)` |
| `p W q` | `p U q` **or** `G p` |
| `p R q` | `¬(¬p U ¬q)` |

### Infinite runs — ultimately periodic (`satisfies_periodic`, `eval_periodic_at`)

`PeriodicTrace { prefix, cycle }` = `prefix` then `cycle` forever. Past the
prefix, atoms repeat with period `L`, so F/G/U are decided by scanning one
full period: `end = max(i, |prefix|) + L`. `X` wraps around the cycle. An
empty `cycle` falls back to LTLf on the prefix.

## Kripke structures

```json
{ "atoms": ["ready", "coin"], "initial": ["idle"],
  "states": [{ "id": "idle", "atoms": ["ready"], "next": ["paid"] }] }
```

`parse_kripke(&Value)` validates: non-empty `states`, unique ids, every
`initial`/`next` id exists. Undeclared atoms used by a state are added with
a **warning**. Empty `initial` warns — every property holds vacuously.
`validate_kripke` applies the same rules to programmatically built specs.

## Model checking (`model_check`)

Enumerates every **simple path** from every initial state up to `depth`
(default `|states|`), and for each path every **lasso**: back-edges into
the current path close a cycle; a dead end closes a self-loop (the system
stutters forever). Each lasso is evaluated under periodic semantics.

Options: `depth`, `path_budget` (default 20 000), `max_counterexamples`
(default 3).

### Verdict honesty (product invariant)

| Verdict | Meaning |
| --- | --- |
| `violated` | ≥1 lasso falsifies the formula — always genuine |
| `holds` | enumeration was exhaustive: `depth ≥ |states|` **and** budget not hit |
| `incomplete` | bounded search found nothing — *unknown*, said out loud |

No code path may report `holds` unless the enumeration was exhaustive.

## Falsification (`falsify`)

Random traces of `length` positions (default 6, max 64), each atom
independently present with p=½, up to `iterations` tries (default 300, max
100 000), PRNG = mulberry32 (same algorithm as the TS engine). First
violating trace is shrunk by delta-debugging: drop positions, then drop
individual atoms, while the trace still violates. `violated: false` means
*not falsified*, never *proven*.

## Coverage (`analyze_coverage`)

Given a model and test traces (state-id sequences), reports covered /
missing states and transitions (`from -> to`), percentages, and state ids
appearing in traces but not the model. Coverage is a negative metric: it
proves absence of testing, never presence of correctness.

## CLI (`fm`)

`fm <tool> '<json-args>'` → JSON result on stdout, exit non-zero on error.
Pass `-` as the args to read the JSON object from stdin (pipe-friendly).
Tools: `check_ltl`, `model_check`, `falsify_property`, `analyze_coverage`,
`explain_concept` — same argument shapes as the sibling project's MCP tools.
