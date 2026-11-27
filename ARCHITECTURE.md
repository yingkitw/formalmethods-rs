# ARCHITECTURE

```
formalmethods (lib)
├── ltl        LTL syntax + evaluation        (src/ltl.rs)
├── model      Kripke structures + checking    (src/model.rs)      [std]
├── fuzz       random falsification + shrink   (src/fuzz.rs)
├── monitor    runtime monitoring (progression) (src/monitor.rs)
├── mutate     mutation-based spec coverage     (src/mutate.rs)    [std]
├── ctl        branching-time CTL checking      (src/ctl.rs)       [std]
├── sweep      depth-sweep: verdict per bound   (src/sweep.rs)     [std]
├── replay     finite replays of lasso CEs      (src/replay.rs)    [std]
├── sat        SAT-backed bounded checking      (src/sat.rs)       [std]
├── concepts   the field guide (prose data)    (src/concepts.rs)
└── fm (bin)   JSON CLI over the same engine   (src/bin/fm.rs)     [std]
```

`model`, `monitor`, and `mutate` depend on `ltl`; `mutate`, `ctl`,
`sweep`, `replay`, and `sat` depend on `model`; `sat` additionally
depends on `varisat` (std-only dep); `concepts` is standalone data;
`fm` depends on everything.
Nothing else couples the modules — the engine is pure functions over
plain data.

## `ltl`

- `Formula` — the AST (`Box`ed enum): future operators `X`/`G`/`F`/`U`/`W`/`R`
  and past-time `Y`/`O`/`H`/`S`. `parse_ltl` is a recursive-descent
  parser over a hand-rolled tokenizer; `print_ltl`/`Display` round-trips.
- `Trace = Vec<State>`, `State = BTreeSet<String>`; `PeriodicTrace` is
  `prefix` + `cycle` for infinite runs.
- Two evaluators: `eval_at` (LTLf, finite) and `eval_periodic_at`
  (ultimately periodic — one full cycle window decides F/G/U). Past
  operators evaluate the unrolled positions `0..=i` — inside the loop the
  past includes earlier iterations.
- `truth_table` / `periodic_truth_table` evaluate every deduplicated
  subformula at every position — the teaching aid and counterexample
  renderer. `bad_positions` = where the run sins.

## `model`

- `KripkeSpec`/`KripkeState` — serde data + `parse_kripke` (JSON in,
  warnings out) / `validate_kripke` (programmatic in).
- `Checker` (private) owns the lasso enumeration: simple-path DFS from
  initial states; back-edges and dead-end stutters each produce one lasso,
  evaluated via `eval_periodic_at`. Verdict honesty lives here — see SPEC.
- `analyze_coverage` — state/transition set arithmetic over test traces.

## `fuzz`

- `Mulberry32` — bit-for-bit port of the TS PRNG, so identical seeds give
  identical campaigns across both engines.
- `falsify` — random-trace campaign; `shrink_trace` — greedy
  delta-debugging (drop positions, then atoms) while still violating.
- `guided: true` — coverage-guided mode: a novelty corpus (state
  signatures, cap 128) plus `mutate_trace` (drop/duplicate/toggle) drives
  generation; the result reports `states_seen`/`states_possible`/
  `corpus_size`. The fresh-trace draw runs every iteration regardless —
  keeps the RNG stream aligned with the TS engine.

## `monitor`

- `Monitor` — Bauer-style formula progression for streaming states:
  `feed(state)` rewrites the residual obligation (residuals stay bare —
  `P(G φ, s) = P(φ, s) ∧ G φ`, never X-wrapped), returns
  `MonitorVerdict::{Satisfied, Violated, Pending}`; terminal verdicts are
  sticky. The consumed prefix is kept as history so past operators
  resolve at each step; `X` residuals carry an `F true` anchor — "a next
  position must exist" — so `X !a` on a one-state log closes as violated.
  `close()` must agree with `satisfies_trace` — enforced by a test
  battery.
- `monitor_trace` — batch form: the same progression rendered as one
  `MonitorStep` per state `{position, atoms, residual, status}`;
  statuses serialize as the TS wire names `met`/`broken`/`owed`.

## `mutate` *(std)*

- Model mutants: `mutants` enumerates single-point faults — `Unlabel`
  (drop an atom), `Relabel` (add a declared atom), `Detransition` (drop
  an edge). `mutation_report` validates each mutant and re-runs the
  formula suite exhaustively; `Killed` on the first `violated`,
  `SurvivedIncomplete` when a check was truncated (verdict honesty
  applies to mutants too), kill rate over valid mutants.
- Formula mutants: `spec_mutants` negates each node position;
  `operator_mutants` applies only strictly-stronger/incomparable
  transforms (`F→G`, `W→U`, `∨→∧`, `∨→operand`, `O→H`, `X→arg`) — a
  transform the original entails could only survive, i.e. report noise.
  `analyze_spec_coverage` re-checks each against the model and scores
  `killed/total` — meaningful only when the base verdict is `holds`.
- Positions are tracked as child-index paths (`paths_of`, `replace_at`)
  so identical subformulas at different positions get distinct mutants.

## `ctl` *(std)*

- `CtlFormula` — branching-time AST: `EX`/`AX`/`EF`/`AF`/`EG`/`AG` and
  `E[φ U ψ]` / `A[φ U ψ]`; `parse_ctl`/`print_ctl` mirror the LTL
  grammar's connectives and conventions.
- `ctl_check` — classic fixpoint labelling: `sat(φ)` computed as a bool
  vector over states; `EU`/`AU` least fixpoints, `EG` greatest fixpoint,
  `EF`/`AF`/`AG` derived. Dead ends get an implicit self-loop (stutter)
  so `AX`/`AG` agree with the LTL checker's convention. Exact on finite
  models — verdict is `holds` iff every initial state is in the set,
  never `incomplete`.

## `sweep` *(std)*

- `analyze_depth` — re-runs `model_check` at every prefix depth
  `1..=|states|`; rows carry `verdict`/`complete`/`checked_lassos`, the
  sweep reports `first_violation_at` (monotone) and `conclusive`. Makes
  `incomplete` visible as a progression, not a verdict.

## `replay` *(std)*

- `counterexample_run` — lasso → `PeriodicTrace` (state ids → atoms).
- `falsifying_window` — unroll prefix + whole loops (default reps
  `[2,3,4,6,8]`) until LTLf still rejects it; `None` when the violation
  needs infinity itself. Turns a model-check counterexample into a
  pasteable regression test.

## `sat` *(std)*

- `sat_check` — bounded model checking via varisat: a symbolic alternative
  to `model_check`'s DFS when models grow. One-hot encoding `x[i][s]` =
  "position `i` is state `s`"; a lasso is positions `0..=k` with back-edge
  `k → l`; one CNF per (length, loop-start) pair at increasing length.
- Formula evaluation is encoded over the same decisive window
  `eval_periodic_at` uses, so `U`/`W`/`R`/`F`/`G` expand to flat
  disjunctions of conjunctions — Tseitin vars only, no fixed-point
  gadgets. Found lassos are re-verified through `eval_periodic_at`
  before being reported.
- Verdict bar identical to `model_check`: `holds` needs `depth ≥ |states|`.

## `concepts`

Static `&'static` data — the same eleven chapters the TypeScript lab
serves, reworded only where it says "this lab" (and noting our native
`ctl_check` where the text says the lab answers compositionally). Powers
`fm explain_concept` and any agent-facing surface built on the crate.
Also exports `SKILLS_GUIDE = include_str!("../SKILLS.md")` — the
doing-guide, served verbatim by `fm read_skills_guide`.

## Dependencies

`serde` + `serde_json` + `varisat` (std-only). The engine itself is
dependency-light by design; JSON is the interchange format (models in,
results out), matching the sibling project's MCP tool schemas.

### Feature split (`std`)

- **default = `std`**: everything above, plus `serde_json` for the CLI's
  JSON plumbing and `varisat` for `sat`.
- **`--no-default-features`**: `no_std` + `alloc` core — `ltl`, `monitor`,
  `concepts`, `fuzz`. `model` and everything built on it (`mutate`, `ctl`,
  `sweep`, `replay`, `sat`) plus the `fm` bin are `std`-gated (`Instant`,
  `HashMap`, `serde_json::Value`, `varisat`). `serde` is configured
  `default-features = false` with `alloc`, so serde impls exist in both
  configurations.

## Porting contract

Behaviour is ported 1:1 from `../formalmethods/src/lib/formal/`: same
grammar and precedence, same LTLf/periodic semantics, same lasso
enumeration order, same shrink strategy, same warning/error wording where
user-visible. When the TS engine changes, mirror it here — and vice versa.

Parity is *verified*, not just claimed: `tests/fixtures/differential.json`
holds ~5 000 (input, verdict) rows computed by the TS engine itself
(`scripts/gen_differential.ts` regenerates it), and `tests/differential.rs`
replays them — `eval_at`, `satisfies_periodic`, and `model_check` verdicts
must agree on every row. A ~50/50 true/false split keeps the corpus
discriminating.
