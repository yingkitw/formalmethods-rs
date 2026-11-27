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
- [x] Output parity polish — `check_ltl` emits the empty-trace `notes`
      warning; `falsify_property` output trimmed to the exact TS shape
- [x] `shrink_trace_for` — formula-aware shrinking (strip atoms the
      formula can't observe — provably safe — then delta-debug);
      extension-only, `falsify` stays semantics-blind
- [x] `print_ltl` associativity fix — equal-prec children now wrap on the
      side the parser re-associates away from (`(a -> b) -> c`,
      `a U (b U c)`); old output reparsed to a *different* AST. TS
      `printLtl` shares the latent bug — deliberate divergence.
- [x] `gen_formula` — randltl-style random formula generator (Spot
      parity); 400-seed parse∘print round-trip property guards the whole
      grammar, past-time ops included
- [x] `atom_density` — opt-in trace-side bias in `falsify` (user-approved
      mature-module edit); `None` keeps the bit-identical ½ campaign
- [x] `sat` — SAT-backed bounded model checking via `varisat` (std-only
      dep, user-approved): one-hot `x[i][s]` encoding, formula evaluated
      over the `eval_periodic_at` decisive window (flat expansions, no
      cyclic definitions); one CNF per (length, loop-start) at increasing
      length → shortest counterexamples; found lassos re-verified through
      `eval_periodic_at`; same `holds` bar as `model_check`; `fm
      sat_check` wired. Verified by a 60-case cross-engine agreement
      battery (20 formulas × 3 presets)
- [x] Differential fixtures — `tests/fixtures/differential.json` holds
      ~5 000 (input, verdict) rows computed by the *actual* TS engine
      (`scripts/gen_differential.ts`, seeded via mulberry32; formula ASTs
      also exercise the serde shape). `tests/differential.rs` replays
      `eval_at`/`satisfies_periodic`/`model_check` on every row —
      ~50/50 verdict split, zero mismatches

## Next

- [ ] (backlog clear — see Brainstorming for candidates)

## Brainstorming (from competitive intel)

- `approaches`-style worked examples (per-language onboarding: native code
  → artifact → LTL mapping); TS ships it as website content — a Rust
  version would be doc examples, not an engine module
- Periodic regeneration of the differential fixture when the TS engine
  changes (or a CI hook into the sibling repo)

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
## Scorecard — 2026-11-22 (pass 2)

| Dimension | Score | Evidence |
|---|---|---|
| Correctness & coverage | 5 | 106 tests; 400-seed gen_formula round-trip property guards every grammar construct |
| Formal fidelity | 5 | `print_ltl` round-trip now *exact* — found+fixed an AST-corruption bug (`(a->b)->c` printed as `a->b->c`); TS shares the latent bug, divergence documented |
| Maintainability | 5 | `with_child` table-collapsed (the flagged weak spot); `gen_formula` is a flat match; clippy 0 both feature sets |
| Docs alignment | 5 | SPEC print-law + gen_formula, TODO, MEMORY harvested same-pass |
| Wiring & ergonomics | 5 | `gen_formula` exported; all parity tools in `fm` (no fm wrapper for it — TS surface has none either) |
| Footprint | 5 | serde + serde_json only |

**Total: 30/30** — the 4→5 maintainability bump is earned by the
`with_child` collapse; residual dense spots (`sat_set`, `apply`) are
test-covered per the mature-module policy. Remaining candidates all
need a user decision (mature-module edit or a new dependency).
## Scorecard — 2026-11-22 (pass 3)

| Dimension | Score | Evidence |
|---|---|---|
| Correctness & coverage | 5 | 116 tests; SAT encoding validated by a 60-case verdict-agreement battery vs `model_check` (20 formulas × 3 presets, every grammar construct) |
| Formal fidelity | 5 | SAT found lassos re-verified through `eval_periodic_at` before reporting; dead-end stutter + past-op windows match the explicit checker |
| Maintainability | 5 | `sat.rs` is one encoder + one driver loop, flat Tseitin helpers; `atom_density` is a single substitution point in `fresh` |
| Docs alignment | 5 | SPEC (sat section + tool list), ARCHITECTURE (module map + dep note), README, MEMORY harvested same-pass |
| Wiring & ergonomics | 5 | `sat_check` in `fm` with preset support and MCP-shaped counterexample; `atom_density` exposed as a `falsify_property` arg |
| Footprint | 4 | `varisat` added (user-approved, std-only — no_std core remains dep-light); `atom_density` is free |

**Total: 29/30** — footprint dips to 4 by design: `varisat` is the only
non-serde dependency and it stays outside the `no_std` core. Residual
brainstorming is content-level (worked examples, differential fixtures).
