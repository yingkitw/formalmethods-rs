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
//! - [`monitor`] — runtime verification: an incremental monitor that feeds
//!   on a state stream and decides as early as the verdict is certain.
//! - [`mutate`] *(std)* — mutation-based specification coverage: does the
//!   formula suite actually catch a broken model?
//! - [`ctl`] *(std)* — branching-time (CTL) checking: `EF`, `AG`,
//!   `E[.. U ..]` and friends via fixpoint labelling — exact, never
//!   `incomplete`.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod concepts;
#[cfg(feature = "std")]
pub mod ctl;
pub mod fuzz;
pub mod ltl;
#[cfg(feature = "std")]
pub mod model;
pub mod monitor;
#[cfg(feature = "std")]
pub mod mutate;

pub use concepts::{CONCEPTS, Concept, ConceptSection, concept_index, get_concept};
#[cfg(feature = "std")]
pub use ctl::{CtlFormula, CtlParseError, CtlResult, ctl_check, parse_ctl, print_ctl};
pub use fuzz::{FalsifyOptions, FalsifyResult, Mulberry32, falsify, shrink_trace};
pub use ltl::{
    Formula, LtlParseError, PeriodicTrace, State, Trace, TruthRow, TruthTable, ast_size,
    collect_atoms, eval_at, eval_periodic_at, parse_ltl, periodic_atoms, periodic_truth_table,
    print_ltl, satisfies_periodic, satisfies_trace, subformulas, trace, truth_table,
};
pub use monitor::{Monitor, MonitorVerdict};
#[cfg(feature = "std")]
pub use model::{
    CounterexampleWindow, CoverageReport, KripkeError, KripkeSpec, KripkeState,
    LassoCounterexample, ModelCheckOptions, ModelCheckResult, Verdict, analyze_coverage,
    model_check, parse_kripke, validate_kripke,
};
#[cfg(feature = "std")]
pub use mutate::{MutantOutcome, MutantReport, Mutation, MutationReport, mutation_report, mutants};
