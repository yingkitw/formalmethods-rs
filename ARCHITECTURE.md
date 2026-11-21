# ARCHITECTURE

```
formalmethods (lib)
├── ltl        LTL syntax + evaluation        (src/ltl.rs)
├── model      Kripke structures + checking    (src/model.rs)
├── fuzz       random falsification + shrink   (src/fuzz.rs)
├── concepts   the field guide (prose data)    (src/concepts.rs)
└── fm (bin)   JSON CLI over the same engine   (src/bin/fm.rs)
```

`model` and `fuzz` depend on `ltl`; `concepts` is standalone data; `fm`
depends on everything. Nothing else couples the modules — the engine is
pure functions over plain data.

## `ltl`

- `Formula` — the AST (`Box`ed enum). `parse_ltl` is a recursive-descent
  parser over a hand-rolled tokenizer; `print_ltl`/`Display` round-trips.
- `Trace = Vec<State>`, `State = BTreeSet<String>`; `PeriodicTrace` is
  `prefix` + `cycle` for infinite runs.
- Two evaluators: `eval_at` (LTLf, finite) and `eval_periodic_at`
  (ultimately periodic — one full cycle window decides F/G/U).
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

## `concepts`

Static `&'static` data — the same ten chapters the TypeScript lab serves,
reworded only where it says "this lab". Powers `fm explain_concept` and any
agent-facing surface built on the crate.

## Dependencies

`serde` + `serde_json` only. The engine itself is dependency-light by
design; JSON is the interchange format (models in, results out), matching
the sibling project's MCP tool schemas.

### Feature split (`std`)

- **default = `std`**: everything above, plus `serde_json` for the CLI's
  JSON plumbing.
- **`--no-default-features`**: `no_std` + `alloc` core — `ltl`, `monitor`,
  `concepts`, `fuzz`. `model` and the `fm` bin are `std`-gated (`Instant`,
  `HashMap`, `serde_json::Value`). `serde` is configured
  `default-features = false` with `alloc`, so serde impls exist in both
  configurations.

## Porting contract

Behaviour is ported 1:1 from `../formalmethods/src/lib/formal/`: same
grammar and precedence, same LTLf/periodic semantics, same lasso
enumeration order, same shrink strategy, same warning/error wording where
user-visible. When the TS engine changes, mirror it here — and vice versa.
