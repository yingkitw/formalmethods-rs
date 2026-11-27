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

## Done — TS parity pass (engine caught up to ../formalmethods)

- [x] Past-time operators `Y`/`O`/`H`/`S` in `ltl` — parser (S at the
      until level, left-assoc), printer, serde kinds, both evaluators;
      unclamped-past semantics (empty-trace `H p` is false — position 0
      still evaluates)
- [x] `unknownTransitions` in `CoverageReport` — undeclared edges taken
      by traces, surfaced not dropped
- [x] Coverage-guided fuzzing — `guided` option: novelty corpus (cap 128)
      + `mutate_trace` + `GuidedCoverage` stats; `fm falsify_property`
      takes `guided`
- [x] Monitor parity — history-resolved past ops, `F true`-anchored `X`
      progression (fixed `X !a` on a 1-state log), `monitor_trace` batch
      API with per-step residuals (`met`/`broken`/`owed` wire names)
- [x] `sweep` module — `analyze_depth`: verdict at every prefix depth
      `1..=|states|`, `first_violation_at`, `conclusive`
- [x] `replay` module — `counterexample_run` + `falsifying_window`
      (default reps `[2,3,4,6,8]`); `fm model_check` counterexamples carry
      `prefix`/`cycle`/`replay`/`truth_table`
- [x] Formula-mutation family in `mutate` — `spec_mutants` (negation per
      position), `operator_mutants` (strictly-stronger/incomparable
      transforms only), `analyze_spec_coverage` (kill score, `None`
      unless base holds)
- [x] `fm` wiring — `monitor_ltl`, `analyze_depth`,
      `analyze_spec_coverage`, `guided` flag
- [x] 11th concept chapter `ctl-vs-ltl` (adapted to mention `ctl_check`)
- [x] `presets` module + `preset` arg on every model-taking `fm` tool
      (vending / vending-eaten / traffic-light — MCP `resolveModel` parity)
- [x] `read_skills_guide` — `SKILLS.md` doing-guide at repo root, served
      verbatim via `SKILLS_GUIDE` (`include_str!`); last tool-surface gap
      vs the TS MCP server closed. `approaches.ts` deliberately skipped —
      it's website lab content, not engine.

## Next

- [ ] (backlog clear — see Brainstorming for candidates)

## Brainstorming (from competitive intel)

- Trace generators with shrinking beyond delta-debugging (e.g. grammar-aware)
- SAT/SMT-backed bounded checking for bigger models (e.g. via `varisat`)
- `approaches`-style worked examples (per-language onboarding: native code
  → artifact → LTL mapping); TS ships it as website content — a Rust
  version would be doc examples, not an engine module

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

## Scorecard — 2026-11-22

| Dimension | Score | Evidence |
|---|---|---|
| Correctness & coverage | 5 | 96 tests green incl. full TS parity numbers (score 66.7, witness paths, depth-sweep verdict rows) |
| Formal fidelity | 5 | past-op unclamped-past semantics verified against TS; `F true` X-anchor fixed a real LTLf bug (`X !a` on 1-state log) |
| Maintainability | 4 | clippy clean both feature sets; `with_child`'s 8 binary arms are repetitive but mechanical |
| Docs alignment | 5 | SPEC (grammar+semantics+tools), ARCHITECTURE (9-module map), README, MEMORY harvested same-pass |
| Wiring & ergonomics | 5 | every capability reachable via `fm` incl. `preset` arg; MCP-shaped outputs (`flipped`, `met/broken/owed`, replay) |
| Footprint | 5 | serde + serde_json only; no_std core intact (sweep/replay/presets std-gated) |

**Total: 29/30** — TS engine parity restored: every capability in
`../formalmethods` now has a Rust twin. Weakest: maintainability (same
note as before — the dense spots are covered and the mature-module
policy prefers tests over rewrites).
