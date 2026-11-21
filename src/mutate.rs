//! Mutation-based specification coverage.
//!
//! A passing model check answers "does this model satisfy these formulas?"
//! but not "would the formulas *catch* a broken model?". Mutation coverage
//! answers the second question: apply one small fault to the Kripke
//! structure, re-run the formula suite, and see whether anything flips.
//! Mutants that every formula still accepts are **survivors** — they point
//! at a hole in the specification.
//!
//! Three mutation operators, applied one at a time:
//!
//! - `Unlabel` — remove one atom from one state's label
//! - `Relabel` — add a declared atom a state didn't have
//! - `Detransition` — remove one edge (the target dead-ends and stutters)
//!
//! All mutants that still validate are checked **exhaustively** — the same
//! honesty rule as [`model_check`][crate::model::model_check]: a mutant is
//! only `Survived` when the enumeration was complete.

use serde::Serialize;

use crate::ltl::{Formula, print_ltl};
use crate::model::{KripkeSpec, ModelCheckOptions, Verdict, model_check, validate_kripke};

/// One single-point fault applied to a [`KripkeSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Mutation {
    /// Remove `atom` from the label of `state`.
    Unlabel { state: String, atom: String },
    /// Add `atom` to the label of `state`.
    Relabel { state: String, atom: String },
    /// Remove the edge `state` → `target`.
    Detransition { state: String, target: String },
}

impl std::fmt::Display for Mutation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mutation::Unlabel { state, atom } => {
                write!(f, "unlabel {state} -{atom}")
            }
            Mutation::Relabel { state, atom } => write!(f, "relabel {state} +{atom}"),
            Mutation::Detransition { state, target } => {
                write!(f, "detransition {state} -/-> {target}")
            }
        }
    }
}

/// What happened to one mutant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MutantOutcome {
    /// At least one formula flipped to `violated` — the spec caught it.
    Killed {
        /// Printed form of the first formula that caught the mutant.
        by: String,
    },
    /// Every formula still reports `holds` — a specification hole.
    Survived,
    /// Every formula reports `holds`/`violated`, but at least one check was
    /// truncated (`incomplete`), so "survived" is not proven.
    SurvivedIncomplete,
    /// The mutation produced a spec that no longer validates.
    Invalid { errors: Vec<String> },
}

/// One mutant and its fate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MutantReport {
    pub mutation: Mutation,
    pub outcome: MutantOutcome,
}

/// Summary of a mutation campaign over a formula suite.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MutationReport {
    pub total: usize,
    pub killed: usize,
    pub survived: usize,
    pub survived_incomplete: usize,
    pub invalid: usize,
    /// Fraction of *valid* mutants killed — the suite's detection power.
    /// `None` when there were no valid mutants.
    pub kill_rate: Option<f64>,
    pub mutants: Vec<MutantReport>,
}

/// Enumerate every single-point mutant of `spec`.
///
/// Order is deterministic: per state (spec order) — unlabel each atom,
/// relabel each missing declared atom, detransition each edge.
pub fn mutants(spec: &KripkeSpec) -> Vec<Mutation> {
    let mut out = Vec::new();
    for s in &spec.states {
        for atom in &s.atoms {
            out.push(Mutation::Unlabel {
                state: s.id.clone(),
                atom: atom.clone(),
            });
        }
        for atom in &spec.atoms {
            if !s.atoms.contains(atom) {
                out.push(Mutation::Relabel {
                    state: s.id.clone(),
                    atom: atom.clone(),
                });
            }
        }
        for target in &s.next {
            out.push(Mutation::Detransition {
                state: s.id.clone(),
                target: target.clone(),
            });
        }
    }
    out
}

/// Apply a mutation, returning the mutated spec.
fn apply(spec: &KripkeSpec, m: &Mutation) -> KripkeSpec {
    let mut spec = spec.clone();
    match m {
        Mutation::Unlabel { state, atom } => {
            if let Some(s) = spec.states.iter_mut().find(|s| s.id == *state) {
                s.atoms.remove(atom);
            }
        }
        Mutation::Relabel { state, atom } => {
            if let Some(s) = spec.states.iter_mut().find(|s| s.id == *state) {
                s.atoms.insert(atom.clone());
            }
        }
        Mutation::Detransition { state, target } => {
            if let Some(s) = spec.states.iter_mut().find(|s| s.id == *state)
                && let Some(i) = s.next.iter().position(|n| n == target)
            {
                s.next.remove(i);
            }
        }
    }
    spec
}

/// Run the mutation campaign: mutate `spec` one fault at a time and
/// re-check every formula exhaustively. A mutant is `Killed` by the first
/// formula (in suite order) that reports `violated`.
pub fn mutation_report(spec: &KripkeSpec, formulas: &[Formula]) -> MutationReport {
    let mutants = mutants(spec);
    let mut report = MutationReport {
        total: mutants.len(),
        killed: 0,
        survived: 0,
        survived_incomplete: 0,
        invalid: 0,
        kill_rate: None,
        mutants: Vec::with_capacity(mutants.len()),
    };
    for mutation in mutants {
        let mut mutated = apply(spec, &mutation);
        let outcome = match validate_kripke(&mut mutated) {
            Err(e) => MutantOutcome::Invalid {
                errors: vec![e.to_string()],
            },
            Ok(_warnings) => {
                let mut killed_by = None;
                let mut incomplete = false;
                for f in formulas {
                    match model_check(f, &mutated, &ModelCheckOptions::default()).verdict {
                        Verdict::Violated => {
                            killed_by = Some(print_ltl(f));
                            break;
                        }
                        Verdict::Incomplete => incomplete = true,
                        Verdict::Holds => {}
                    }
                }
                match killed_by {
                    Some(by) => MutantOutcome::Killed { by },
                    None if incomplete => MutantOutcome::SurvivedIncomplete,
                    None => MutantOutcome::Survived,
                }
            }
        };
        match &outcome {
            MutantOutcome::Killed { .. } => report.killed += 1,
            MutantOutcome::Survived => report.survived += 1,
            MutantOutcome::SurvivedIncomplete => report.survived_incomplete += 1,
            MutantOutcome::Invalid { .. } => report.invalid += 1,
        }
        report.mutants.push(MutantReport { mutation, outcome });
    }
    let valid = report.total - report.invalid;
    report.kill_rate = (valid > 0).then(|| report.killed as f64 / valid as f64);
    report
}
