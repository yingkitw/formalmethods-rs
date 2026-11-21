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
- [x] `fm` reads args from stdin when the JSON arg is `-` (pipe-friendly)
- [x] Property patterns helper — `Formula::{always, eventually,
      responds_to}` constructors + `From<&str>`/`From<String>` for atoms

- [x] Runtime-monitor API — `monitor::Monitor` feeds states incrementally,
      decides early, `close()` agrees with `satisfies_trace` (asserted by
      tests on 18 formula×trace cases)
- [x] `no_std`+`alloc` core — `ltl`/`monitor`/`concepts`/`fuzz` build under
      `--no-default-features`; `model` and `fm` stay `std`-gated (Instant,
      HashMap, serde_json). Verified: `cargo build|test|clippy
      --no-default-features` green
- [x] Mutation-based spec coverage — `mutate` module: `mutants()` enumerates
      single-point faults (unlabel/relabel/detransition),
      `mutation_report()` re-checks the formula suite exhaustively and
      reports killed/survived/invalid + kill rate
- [x] CTL branching properties — `ctl` module: `CtlFormula` AST, `parse_ctl`
      (E[.. U ..]/A[.. U ..], EX/AX/EF/AF/EG/AG), fixpoint labelling
      checker honoring dead-end stuttering; `fm ctl_check` wired

## Next

- [ ] (backlog clear — see Brainstorming for candidates)

## Brainstorming (from competitive intel)

- Past-time LTL operators (`Y`, `O`, `H`, `S`) for log checking
- Trace generators with shrinking beyond delta-debugging (e.g. grammar-aware)
- SAT/SMT-backed bounded checking for bigger models (e.g. via `varisat`)

## Scorecard

| Date | Correctness | Formal fidelity | Maintainability | Docs | CLI | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-11-21 | 5 | 5 | 4 | 4 | 4 | Initial port; all TS tests mirrored and green |

## Scorecard — 2026-11-21

| Dimension | Score | Evidence |
|---|---|---|
| Correctness & coverage | 5 | 56 tests green; LTLf+periodic, lasso, fuzz, monitor, mutate, CTL all exercised |
| Formal fidelity | 5 | dead-end stuttering consistent across LTL and CTL; verdict honesty held in mutate (`SurvivedIncomplete`) |
| Maintainability | 4 | clippy clean both feature sets; `sat_set`/`apply` are dense but single-purpose |
| Docs alignment | 5 | README/SPEC/ARCHITECTURE/TODO updated same-pass |
| Wiring & ergonomics | 5 | `ctl`+`mutate` re-exported and reachable via `fm ctl_check` / `fm mutation_report` |
| Footprint | 5 | serde + serde_json only; no_std core preserved |

**Total: 29/30** — up from previous pass. Weakest: maintainability
(fixpoint engine and `until_or_unary` are the densest code; acceptable —
covered by 12 CTL tests, mature-module policy prefers coverage).
