# formalmethods

Formal methods for software testing and evaluation, in Rust.

Write one temporal-logic sentence — `G (request -> F response)` — and it
becomes your test oracle: evaluated on finite traces (LTLf), model-checked
exhaustively over Kripke structures, attacked by a seeded fuzzer, and
measured for coverage.

This crate is the Rust port of the engine behind
[formalmethods.dev](../formalmethods) — same semantics, same verdicts.

## What's inside

| Module | What it is |
| --- | --- |
| `ltl` | LTL parser, printer, and **two** evaluators: finite-trace (LTLf) and ultimately-periodic (infinite) semantics — future (`X`/`G`/`F`/`U`/`W`/`R`) **and** past-time (`Y`/`O`/`H`/`S`) operators, plus truth tables and a serde JSON AST |
| `model` | Kripke structures, lasso-based model checker (exhaustive or depth-bounded), state/transition coverage analysis |
| `fuzz` | Property-based falsification: seeded random traces (`Mulberry32`), optional coverage-guided corpus mode, + delta-debugging shrinker |
| `monitor` | Runtime verification: incremental `Monitor` that consumes a stream of states and decides the moment the verdict is inevitable; `monitor_trace` renders the residual obligation step by step |
| `mutate` | Mutation-based spec coverage, both directions: inject single faults into the **model** and re-check the formula suite — or mutate the **formula** (negate, strengthen, drop) and re-check the model |
| `ctl` | Branching-time (CTL) checking: `EF`, `AG`, `EX`, `E[.. U ..]` via fixpoint labelling — exact on finite models, never `incomplete` |
| `sweep` | Depth sweeps: the model-check verdict at every prefix bound `1..=|states|` — `incomplete` as a progression, not a verdict |
| `replay` | Finite replays of infinite counterexamples: unroll a lasso until LTLf still rejects it — a pasteable regression test |
| `sat` | SAT-backed bounded model checking (varisat): one-hot lasso encoding, shortest-first counterexamples, same `holds` bar as `model_check` — re-verified through the evaluator before reporting |
| `concepts` | The field guide: eleven chapters of explanation for agent-facing tooling |
| `fm` (bin) | CLI front-end with one subcommand per capability, JSON in → JSON out |

## Library

```toml
[dependencies]
formalmethods = "0.1"
```

```rust
use formalmethods::{parse_ltl, satisfies_trace, trace};

let f = parse_ltl("G (request -> F response)").unwrap();
assert!(satisfies_trace(&f, &trace(&[&["request"], &["response"]])));
```

## CLI

```sh
cargo install --path .

fm check_ltl        '{"formula":"G (request -> F response)","trace":[["request"],["timeout"]]}'
fm monitor_ltl      '{"formula":"G (a -> F b)","trace":[["a"],["b"]]}'
fm model_check      '{"formula":"G (coin -> F dispense)","model":{...}}'
fm sat_check        '{"formula":"G (coin -> F dispense)","model":{...}}'  # SAT backend
fm ctl_check        '{"formula":"AG (coin -> AF vend)","model":{...}}'
fm analyze_depth    '{"formula":"G (coin -> F dispense)","model":{...}}'
fm falsify_property '{"formula":"G (a -> F b)","atoms":["a","b"],"seed":42,"guided":true}'
fm analyze_coverage '{"model":{...},"traces":[["idle","paid","vend"]]}'
fm analyze_spec_coverage '{"formula":"G (coin -> F dispense)","model":{...}}'
fm explain_concept  '{"concept":"ltl"}'     # or no arg for the topic index
fm read_skills_guide                        # the doing-guide (SKILLS.md), verbatim
```

Every model-taking tool also accepts `"preset":"vending"`,
`"vending-eaten"`, or `"traffic-light"` instead of a `model` object.

## Two semantics, one grammar

- **Finite traces** (test cases, logs): `X` is false at the last position;
  `F`/`G` quantify over what you saw. Used by `satisfies_trace`,
  `truth_table`, and the fuzzer.
- **Infinite runs** (Kripke models): every behaviour is ultimately periodic
  (prefix + loop). Used by `model_check` via lasso enumeration. Dead ends
  **stutter forever**, so liveness catches hangs.

## Verdict honesty

Every answer is labelled with exactly what it proves:

- `holds` — the lasso enumeration was exhaustive (full depth, no truncation)
- `violated` — with a genuine counterexample lasso and its truth table
- `incomplete` — the search was bounded (depth limit or path budget)
- falsification `violated: false` — means *not falsified in the campaign*,
  never *proven*

## Development

```sh
cargo build
cargo test            # engine tests (parser, both semantics, checker, fuzzer)
cargo run --example vending   # end-to-end tour
cargo clippy --all-targets    # lint-clean

# no_std core: ltl + monitor + concepts + fuzz (needs alloc)
cargo build --no-default-features
cargo test  --no-default-features
```

Docs: `SPEC.md` (grammar + verdict semantics), `ARCHITECTURE.md` (module
map), `TODO.md` (roadmap).
