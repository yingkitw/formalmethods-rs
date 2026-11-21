# TODO

## Done

- [x] `ltl` module — parser, printer, subformulas/atom collection, LTLf +
      ultimately-periodic evaluators, truth tables
- [x] `model` module — `parse_kripke`/`validate_kripke`, lasso-based
      `model_check` with honest verdicts, `analyze_coverage`
- [x] `fuzz` module — `Mulberry32`, `falsify`, `shrink_trace`
- [x] `concepts` module — the ten-chapter field guide as static data
- [x] `fm` CLI — `check_ltl`, `model_check`, `falsify_property`,
      `analyze_coverage`, `explain_concept`
- [x] Test suite ported from the TS engine (`tests/ltl.rs`, `tests/model.rs`,
      `tests/fuzz.rs`) + `examples/vending.rs`
- [x] `cargo build` / `cargo test` / `cargo clippy --all-targets` clean

## Done (cont.)

- [x] `Formula` serde round-trip — TS-compatible tagged JSON AST
      (`{kind,arg|left,right|name}`), serde impl in `src/ltl.rs`
- [x] Cargo.toml metadata (repository/homepage/documentation) — fixes the
      `cargo publish` warning

## Next

- [ ] `fm` reads args from stdin when the JSON arg is `-` (pipe-friendly)
- [ ] Property patterns helper (`always`, `eventually`, `responds_to`) —
      constructors for the common spec shapes
- [ ] Runtime-monitor API: incremental `eval_at` feed for streaming logs
      (consume events one at a time instead of a whole `Trace`)
- [ ] `#[no_std]`-able core (`ltl` without serde) behind a feature flag

## Brainstorming (from competitive intel)

- Past-time LTL operators (`Y`, `O`, `H`, `S`) for log checking
- CTL-style branching properties on Kripke structures
- Mutation-based spec coverage: does the test suite catch a mutated model?
- Trace generators with shrinking beyond delta-debugging (e.g. grammar-aware)
- SAT/SMT-backed bounded checking for bigger models (e.g. via `varisat`)

## Scorecard

| Date | Correctness | Formal fidelity | Maintainability | Docs | CLI | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-11-21 | 5 | 5 | 4 | 4 | 4 | Initial port; all TS tests mirrored and green |
