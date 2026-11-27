# SKILLS.md — doing formal methods with the crate

**Name**: formalmethods
**Description**: Use when the task is to specify, test, or verify system behaviour with formal methods — writing LTL properties, building Kripke models, model checking, falsifying, measuring coverage, or generating tests from specs. Triggers: "write a property", "model check", "find a counterexample", "is this spec satisfiable", "coverage of my tests", "property-based test this", or any mention of LTL/temporal logic/verification. Also use when an agent needs to call this crate's `fm` tools.

---

## The one idea

Testing with formal methods = **write the obligation once, let machines generate and judge the cases**.

```
obligation (LTL)  →  cases (runs)  →  verdict (holds / violated / incomplete)
```

Everything below is a recipe for one of those three steps.

## Step 1 — Write the property

LTL grammar (as this crate parses it):

```
p ::= atom | true | false | !p | p && p | p || p | p -> p | p <-> p
    | X p | G p | F p | p U p | p W p | p R p
    | Y p | O p | H p | p S p                    (atoms: [A-Za-z_][A-Za-z0-9_]*)
```

Precedence loosest→tightest: `<->`, `->`, `||`, `&&`, then `U W R S` (left-assoc), then `! X G F Y O H`.

`Y` (yesterday), `O` (once), `H` (historically), `S` (since) look *backward* — both semantics agree on them. `Y` is strong: false at position 0.

Property patterns and when to reach for them:

| You want to say | Write | Kind |
| --- | --- | --- |
| nothing bad ever happens | `G !bad` | safety |
| something good eventually happens | `F good` / `G F good` | liveness |
| every request is answered | `G (request -> F response)` | response |
| grant only when ready | `G (grant -> ready)` | invariant |
| after grant, ready next step | `G (grant -> X ready)` | step |
| `p` holds until `q` takes over | `p U q` (must happen) / `p W q` (may never) | precedence |
| alternate forever | `G (a -> X (!a U b))` | rhythm |
| grant only ever follows a request | `G (grant -> O request)` | precedence |

Rules of thumb:
- One obligation per formula. If you wrote `&&` between two independent thoughts, you will get two tools' worth of value by checking them separately.
- Say *when*, not *how*. `F response` — not `retry then response`. The model owns "how"; the property owns "when".
- If you find yourself writing `G (!a && !b && ...)`, ask whether `G !(a || b || ...)` or an auxiliary atom reads better. Future-you reads these.

## Step 2 — Pick the instrument

The checking tools answer different questions. Pick by question, not by habit:

| Question | Tool / instrument | Cost |
| --- | --- | --- |
| Does **this exact run** satisfy the property? | `check_ltl` | cheap, finite semantics (LTLf) |
| **When and why** does the property fail as events arrive? | `monitor_ltl` | cheap, one residual per state |
| Does **every possible run** of the model satisfy it? | `model_check` | exhaustive over finite models |
| Does it hold on **every branch** (not every run)? | `ctl_check` | exact CTL fixpoint, never incomplete |
| **When does a bound** become conclusive? | `analyze_depth` | one check per depth 1..\|S\| |
| Can **random runs** break it? | `falsify_property` | cheap, shrinks what it finds |
| Which **states/transitions did my tests never touch**? | `analyze_coverage` | cheap, negative metric |
| Which **clauses of my passing spec** actually tested something? | `analyze_spec_coverage` | one re-check per subformula |
| Would **my formula suite** catch a broken model? | `mutation_report` | one re-check per model fault |

Decision recipe:
- You have a **log/test trace** and a property → `check_ltl`. Truth table tells you *which step* broke it; `monitor_ltl` tells you *when the break became inevitable* — the residual obligation after each state (met / broken / owed).
- You have a **model** (states, transitions, labels) → `model_check`. Counterexample = a ready-made failing test: each lasso carries a `replay` trace that still fails under `check_ltl` (`null` when only the infinite loop witnesses it). No model yet? Every model-taking tool (`model_check`, `ctl_check`, `analyze_depth`, `analyze_coverage`, `analyze_spec_coverage`, `mutation_report`) accepts `preset` instead of `model` — `vending` (correct), `vending-eaten` (the coin-eating bug), `traffic-light` (stuck red).
- **Branching-time** questions — "is a hang reachable", "safe on every branch" — are CTL, not LTL: `ctl_check` with `EF`, `AG`, `E[.. U ..]`. Exact on finite models, so no `incomplete` row ever.
- Model too big to check? Bound the depth (`model_check` with `depth`) — bounded model checking: violations are real, "incomplete" is honest. `analyze_depth` runs the whole sweep at once — one row per bound, the last is exhaustive; a violation once surfaced never recedes.
- No model, just a spec you distrust → `falsify_property` over the atoms. A surviving campaign is **evidence, not proof**. Add `guided: true` for a coverage-seeking campaign — it mutates a corpus of novel traces and reports how much of the 2^|atoms| state space it touched.
- Wrote tests against a model → `analyze_coverage` with the traces. It proves gaps, never correctness — and `unknown_transitions` names the edges your traces took that the model does not even declare.
- Spec `holds` but feels thin → `analyze_spec_coverage`. Every clause is mutated — negated, strengthened (`F→G`, `W→U`, `∨→∧`, `O→H`), or operand-dropped — and re-checked; a **survived** mutant is an obligation no behaviour distinguishes — the spec was vacuous there. `kind` distinguishes an untested *clause* (negation survives) from an unexercised *operator* (the stronger form also holds — e.g. `F p` where `G p` is true anyway). Killed mutants come with the witness lasso.
- The dual worry — formula suite feels thin → `mutation_report`. The model is mutated one fault at a time (unlabel / relabel / detransition) and the whole suite re-checked; survivors are faults no formula catches — holes in the spec.
- Don't know the vocabulary → `explain_concept` (no args: index; with `concept`: chapter).
- Need the whole manual → `read_skills_guide` (returns this very file, verbatim).

## Step 3 — Read the verdict honestly

Every tool labels exactly what it proved. Quote it verbatim; never upgrade:

- **`holds`** — only from exhaustive enumeration (depth = every state, no truncation). Means: *this model* cannot violate it. As strong as the model is faithful.
- **`violated`** — sound even when truncated: the counterexample lasso is a genuine failing run. Turn it into a regression test.
- **`incomplete`** — depth or path budget stopped the search. It means *unknown*, not *passing*.
- **`not falsified`** — random traces stayed lucky. Evidence, not proof.

A failing fuzzer campaign plus a `holds` model check on the same formula is the dream result: the machine found nothing you can't explain, and the exhaustive search found nothing at all.

## Step 4 — Build the model (when you need one)

Kripke structure, as JSON:

```json
{
  "atoms": ["ready", "coin", "dispense"],
  "initial": ["idle"],
  "states": [
    { "id": "idle",  "atoms": ["ready"],    "next": ["paid"] },
    { "id": "paid",  "atoms": ["coin"],     "next": ["vend"] },
    { "id": "vend",  "atoms": ["dispense"], "next": ["idle"] }
  ]
}
```

- `atoms` = the vocabulary; label each state with the ones true there.
- `next` = may-transitions: **nondeterminism is a feature** — checking the structure checks every resolution (that's the adversary).
- A state with no `next` **stutters forever** — that's how `F x` catches hangs.
- Strip everything irrelevant: no variables, no timers, only what the property mentions. If the model is big, it's wrong.

## Calling the tools

Two surfaces, one engine. The `fm` CLI takes one JSON argument per tool (the same shapes as the MCP tools of the sibling project); pass `-` to read the JSON from stdin:

```sh
fm check_ltl '{"formula":"G (request -> F response)","trace":[["request"],["timeout"]]}'
fm model_check '{"formula":"G (coin -> F dispense)","preset":"vending"}'
echo '{"formula":"F vend","preset":"vending"}' | fm model_check -
```

The library is the same functions without the JSON shell:

```rust
use formalmethods::{parse_ltl, satisfies_trace, trace};

let f = parse_ltl("G (request -> F response)").unwrap();
assert!(satisfies_trace(&f, &trace(&[&["request"], &["response"]])));
```

- Bad args → a non-zero exit with the error on stderr.
- Seed everything: `falsify_property` returns the seed it used — record it in the bug report.

## Recipes

**Spec-first feature**: write property → `model_check` against the model → take counterexamples as tests → `analyze_coverage` → `analyze_spec_coverage` on the passing spec (survivors = clauses that tested nothing) → ship the model + property + generated traces in the repo.

**Triage a flaky bug**: write the property you *believe* → `falsify_property` with a fixed seed → shrink output is your minimal reproduction → `check_ltl` on the shrunken trace shows exactly which step sins.

**Check a log/CI run**: same property, `check_ltl` over the trace of events for the verdict — `monitor_ltl` for the monitor's state per step: watch `owed` obligations accumulate and pin the state where the run first sins.

**Hunt liveness bugs**: `G F something` + a model with a stuttering dead end. If the checker says `violated` with a lasso, the loop in the counterexample *is* the hang.

## Honesty checklist (before you report a result)

- [ ] Named the semantics: LTLf (finite) or infinite (lasso)?
- [ ] Quoted the tool's verdict verbatim — no upgrades?
- [ ] Seeded and recorded every random campaign?
- [ ] Turned every counterexample into a kept test?
- [ ] Labelled bounded/unfalsified results as evidence, not proof?

> If you caught yourself saying "it passes" about an `incomplete` or `not falsified` result — stop, rerun, say what actually happened. The crate is built so you never have to lie about a verdict; use that.
