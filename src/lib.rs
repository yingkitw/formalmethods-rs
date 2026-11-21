//! # formalmethods
//!
//! Formal methods for software testing and evaluation. Write one
//! temporal-logic sentence — `G (request -> F response)` — and it becomes
//! your test oracle: evaluated on finite traces, model-checked exhaustively
//! over Kripke structures, attacked by a seeded fuzzer, and measured for
//! coverage.
//!
//! ## Modules
//!
//! - [`ltl`] — LTL parser, printer, and two evaluators: finite-trace (LTLf)
//!   and ultimately-periodic (infinite) semantics, plus truth tables.
//! - [`model`] — Kripke structures, lasso-based model checker (exhaustive or
//!   depth-bounded), state/transition coverage analysis.
//! - [`fuzz`] — property-based falsification: seeded random traces and a
//!   delta-debugging shrinker.
//! - [`concepts`] — the field guide: ten chapters of explanation, for
//!   agent-facing tooling.
//!
//! ## Verdict honesty
//!
//! Every verdict is labelled with exactly what it proves: `holds` only when
//! the lasso enumeration was exhaustive, `violated` with a genuine
//! counterexample, `incomplete` when the search was bounded. Nothing
//! overstates.
//!
//! ## Quick start
//!
//! ```
//! use formalmethods::{parse_ltl, satisfies_trace, trace};
//!
//! let f = parse_ltl("G (request -> F response)").unwrap();
//! assert!(satisfies_trace(&f, &trace(&[&["request"], &["response"]])));
//! assert!(!satisfies_trace(&f, &trace(&[&["request"], &[]])));
//! ```

pub mod concepts;
pub mod fuzz;
pub mod ltl;
pub mod model;
pub mod monitor;

pub use concepts::{CONCEPTS, Concept, ConceptSection, concept_index, get_concept};
pub use fuzz::{FalsifyOptions, FalsifyResult, Mulberry32, falsify, shrink_trace};
pub use ltl::{
    Formula, LtlParseError, PeriodicTrace, State, Trace, TruthRow, TruthTable, ast_size,
    collect_atoms, eval_at, eval_periodic_at, parse_ltl, periodic_atoms, periodic_truth_table,
    print_ltl, satisfies_periodic, satisfies_trace, subformulas, trace, truth_table,
};
pub use monitor::{Monitor, MonitorVerdict};
pub use model::{
    CounterexampleWindow, CoverageReport, KripkeError, KripkeSpec, KripkeState,
    LassoCounterexample, ModelCheckOptions, ModelCheckResult, Verdict, analyze_coverage,
    model_check, parse_kripke, validate_kripke,
};
