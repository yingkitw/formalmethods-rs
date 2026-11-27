//! Mutation-based specification coverage — in both directions.
//!
//! **Model mutants** ([`mutants`], [`mutation_report`]): a passing model
//! check answers "does this model satisfy these formulas?" but not "would
//! the formulas *catch* a broken model?". Apply one small fault to the
//! Kripke structure, re-run the formula suite, and see whether anything
//! flips. Mutants that every formula still accepts are **survivors** —
//! they point at a hole in the specification.
//!
//! - `Unlabel` — remove one atom from one state's label
//! - `Relabel` — add a declared atom a state didn't have
//! - `Detransition` — remove one edge (the target dead-ends and stutters)
//!
//! **Formula mutants** ([`spec_mutants`], [`operator_mutants`],
//! [`analyze_spec_coverage`]): the dual question — a property that holds
//! is not automatically a *tested* property. Mutate each subformula
//! (negation, operator-strengthening, operand drop) and re-check the
//! model. Mutants the model cannot detect are obligations no behaviour
//! distinguishes — gaps in the spec, honestly reported.
//!
//! All mutants that still validate are checked **exhaustively** — the same
//! honesty rule as [`model_check`][crate::model::model_check]: a mutant is
//! only `Survived` when the enumeration was complete.

use serde::Serialize;

use crate::ltl::{Formula, ast_size, print_ltl};
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

// ---------------------------------------------------------------------------
// Formula mutants — spec coverage of a passing property
// ---------------------------------------------------------------------------

/// Which family produced a formula mutant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecMutantKind {
    /// The node was wrapped in `!`.
    Negation,
    /// The node was replaced by a strictly stronger operator
    /// (`F→G`, `W→U`, `∨→∧`, `O→H`).
    Strengthen,
    /// The node was replaced by one of its operands (`∨→operand`,
    /// `X→arg`).
    Drop,
}

impl SpecMutantKind {
    /// The TS engine's kind string — also the sort key.
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecMutantKind::Negation => "negation",
            SpecMutantKind::Strengthen => "strengthen",
            SpecMutantKind::Drop => "drop",
        }
    }
}

/// One mutated formula: which subformula changed, into what whole
/// property, and by which family.
#[derive(Debug, Clone, Serialize)]
pub struct SpecMutant {
    /// The subformula that was mutated, printed (`A → B` for transforms).
    pub target: String,
    /// The whole mutated property, printed.
    pub mutation: String,
    /// The mutated property, ready for the checker.
    pub formula: Formula,
    /// Which family produced the mutant.
    pub kind: SpecMutantKind,
    /// [`ast_size`] of the replaced node — for deterministic
    /// smallest-first ordering.
    pub size: usize,
}

fn children_of(f: &Formula) -> Vec<&Formula> {
    match f {
        Formula::True | Formula::False | Formula::Atom(_) => vec![],
        Formula::Not(arg)
        | Formula::Next(arg)
        | Formula::Globally(arg)
        | Formula::Finally(arg)
        | Formula::Yesterday(arg)
        | Formula::Once(arg)
        | Formula::Historically(arg) => vec![arg],
        Formula::And(l, r)
        | Formula::Or(l, r)
        | Formula::Implies(l, r)
        | Formula::Iff(l, r)
        | Formula::Until(l, r)
        | Formula::Weak(l, r)
        | Formula::Release(l, r)
        | Formula::Since(l, r) => vec![l, r],
    }
}

/// A binary `Formula` variant seen as its 2-tuple constructor.
type BinaryCtor = fn(Box<Formula>, Box<Formula>) -> Formula;

fn with_child(f: &Formula, i: usize, c: Formula) -> Formula {
    match f {
        Formula::True | Formula::False | Formula::Atom(_) => f.clone(),
        Formula::Not(_) => Formula::Not(Box::new(c)),
        Formula::Next(_) => Formula::Next(Box::new(c)),
        Formula::Globally(_) => Formula::Globally(Box::new(c)),
        Formula::Finally(_) => Formula::Finally(Box::new(c)),
        Formula::Yesterday(_) => Formula::Yesterday(Box::new(c)),
        Formula::Once(_) => Formula::Once(Box::new(c)),
        Formula::Historically(_) => Formula::Historically(Box::new(c)),
        // Every binary variant is a 2-tuple constructor — pick it, then
        // rebuild with `c` in slot `i`.
        _ => {
            let (ctor, l, r): (BinaryCtor, &Formula, &Formula) = match f {
                Formula::And(l, r) => (Formula::And, l, r),
                Formula::Or(l, r) => (Formula::Or, l, r),
                Formula::Implies(l, r) => (Formula::Implies, l, r),
                Formula::Iff(l, r) => (Formula::Iff, l, r),
                Formula::Until(l, r) => (Formula::Until, l, r),
                Formula::Weak(l, r) => (Formula::Weak, l, r),
                Formula::Release(l, r) => (Formula::Release, l, r),
                Formula::Since(l, r) => (Formula::Since, l, r),
                _ => unreachable!("children_of only yields unary/binary nodes"),
            };
            if i == 0 {
                ctor(Box::new(c), Box::new(r.clone()))
            } else {
                ctor(Box::new(l.clone()), Box::new(c))
            }
        }
    }
}

/// Paths to every node (child indices from the root), pre-order.
fn paths_of(f: &Formula) -> Vec<Vec<usize>> {
    fn go(f: &Formula, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        out.push(path.clone());
        for (i, c) in children_of(f).iter().enumerate() {
            path.push(i);
            go(c, path, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    go(f, &mut Vec::new(), &mut out);
    out
}

fn node_at<'a>(f: &'a Formula, path: &[usize]) -> &'a Formula {
    let mut n = f;
    for &i in path {
        n = children_of(n)[i];
    }
    n
}

/// The formula with the node at `path` replaced by `new`.
fn replace_at(f: &Formula, path: &[usize], new: &Formula) -> Formula {
    match path.split_first() {
        None => new.clone(),
        Some((&i, rest)) => {
            let c = replace_at(children_of(f)[i], rest, new);
            with_child(f, i, c)
        }
    }
}

fn sort_mutants(a: &SpecMutant, b: &SpecMutant) -> std::cmp::Ordering {
    a.size
        .cmp(&b.size)
        .then_with(|| a.target.cmp(&b.target))
        .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
}

/// One mutant per node position: the formula with that node wrapped in
/// `!`. Sorted smallest target first, root last.
pub fn spec_mutants(f: &Formula) -> Vec<SpecMutant> {
    let mut out: Vec<SpecMutant> = paths_of(f)
        .iter()
        .map(|path| {
            let node = node_at(f, path);
            let formula = replace_at(f, path, &Formula::Not(Box::new(node.clone())));
            SpecMutant {
                target: print_ltl(node),
                mutation: print_ltl(&formula),
                formula,
                kind: SpecMutantKind::Negation,
                size: ast_size(node),
            }
        })
        .collect();
    out.sort_by(sort_mutants);
    out
}

/// The transform each node kind admits — see the entailment note on
/// [`operator_mutants`].
fn swaps(node: &Formula) -> Vec<(SpecMutantKind, Formula)> {
    match node {
        Formula::Finally(arg) => {
            vec![(SpecMutantKind::Strengthen, Formula::Globally(arg.clone()))]
        }
        Formula::Weak(l, r) => {
            vec![(SpecMutantKind::Strengthen, Formula::Until(l.clone(), r.clone()))]
        }
        Formula::Or(l, r) => vec![
            (SpecMutantKind::Strengthen, Formula::And(l.clone(), r.clone())),
            (SpecMutantKind::Drop, (**l).clone()),
            (SpecMutantKind::Drop, (**r).clone()),
        ],
        Formula::Next(arg) => vec![(SpecMutantKind::Drop, (**arg).clone())],
        Formula::Once(arg) => {
            vec![(SpecMutantKind::Strengthen, Formula::Historically(arg.clone()))]
        }
        _ => vec![],
    }
}

/// Mutants that *strengthen* a node or drop a sibling instead of negating
/// it — a second family answering "is the operator itself exercised?",
/// not just "is the clause load-bearing?".
///
/// Discipline: a mutant the original *entails* is useless — `G p` holding
/// guarantees `F p` also holds, so a `G→F` swap can only ever survive and
/// reports noise, not a gap. Every transform is strictly stronger than
/// (`F→G`, `W→U`, `∨→∧`, `∨→operand`, `O→H`) or incomparable to (`X→drop`)
/// the node it replaces, so `Survived` means the model genuinely cannot
/// distinguish the weak spec from the strong one.
pub fn operator_mutants(f: &Formula) -> Vec<SpecMutant> {
    let mut out = Vec::new();
    for path in paths_of(f) {
        let node = node_at(f, &path);
        for (kind, to) in swaps(node) {
            let formula = replace_at(f, &path, &to);
            out.push(SpecMutant {
                target: format!("{} → {}", print_ltl(node), print_ltl(&to)),
                mutation: print_ltl(&formula),
                formula,
                kind,
                size: ast_size(node),
            });
        }
    }
    out.sort_by(sort_mutants);
    out
}

/// How a re-checked mutant resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutantStatus {
    /// The model detects the flip — the clause is load-bearing.
    Killed,
    /// The model cannot tell — a spec gap.
    Survived,
    /// Undecided within the bounds.
    Unknown,
}

/// The lasso that kills a mutant — evidence the clause is load-bearing.
#[derive(Debug, Clone, Serialize)]
pub struct SpecWitness {
    pub path: Vec<String>,
    pub cycle_start: usize,
}

/// A mutant plus its re-check outcome.
#[derive(Debug, Clone, Serialize)]
pub struct SpecCoverageRow {
    pub target: String,
    pub mutation: String,
    pub formula: Formula,
    pub kind: SpecMutantKind,
    pub size: usize,
    pub verdict: Verdict,
    pub status: MutantStatus,
    pub witness: Option<SpecWitness>,
}

/// The result of [`analyze_spec_coverage`].
#[derive(Debug, Clone, Serialize)]
pub struct SpecCoverage {
    pub base_verdict: Verdict,
    pub mutants: Vec<SpecCoverageRow>,
    pub killed: usize,
    pub survived: usize,
    pub unknown: usize,
    /// Mutants built and re-checked (negations + operator transforms).
    pub total: usize,
    /// `killed / total` as a percent with one decimal; `None` when the
    /// base verdict isn't `holds`.
    pub score: Option<f64>,
    pub notes: Vec<String>,
}

/// Flip every subformula of `formula`, re-check the model, and report
/// which mutants survive. Meaningful only when the base property holds —
/// a failing or undecided base makes "survived" meaningless, so we say so
/// and stop.
pub fn analyze_spec_coverage(
    formula: &Formula,
    model: &KripkeSpec,
    opts: &ModelCheckOptions,
    max_mutants: Option<usize>,
) -> SpecCoverage {
    let max_mutants = max_mutants.unwrap_or(64);
    let base = model_check(formula, model, opts);
    let mut notes = base.notes.clone();
    let empty = |notes: Vec<String>| SpecCoverage {
        base_verdict: base.verdict,
        mutants: vec![],
        killed: 0,
        survived: 0,
        unknown: 0,
        total: 0,
        score: None,
        notes,
    };
    if base.verdict != Verdict::Holds {
        notes.insert(
            0,
            "spec coverage measures which obligations of a passing property the model actually tests — the base check must hold first".to_string(),
        );
        return empty(notes);
    }

    let mut all = spec_mutants(formula);
    all.extend(operator_mutants(formula));
    all.sort_by(sort_mutants);
    let flippable = all.len();
    let mutants: Vec<SpecMutant> = all.into_iter().take(max_mutants).collect();
    if mutants.len() < flippable {
        notes.push(format!(
            "formula has {flippable} flippable positions — only the first {} were mutated",
            mutants.len()
        ));
    }

    let rows: Vec<SpecCoverageRow> = mutants
        .into_iter()
        .map(|m| {
            let r = model_check(&m.formula, model, opts);
            let status = match r.verdict {
                Verdict::Violated => MutantStatus::Killed,
                Verdict::Holds => MutantStatus::Survived,
                _ => MutantStatus::Unknown,
            };
            let witness = r.counterexamples.first().map(|ce| SpecWitness {
                path: ce.path.clone(),
                cycle_start: ce.cycle_start,
            });
            SpecCoverageRow {
                target: m.target,
                mutation: m.mutation,
                formula: m.formula,
                kind: m.kind,
                size: m.size,
                verdict: r.verdict,
                status,
                witness,
            }
        })
        .collect();

    let killed = rows.iter().filter(|r| r.status == MutantStatus::Killed).count();
    let survived = rows.iter().filter(|r| r.status == MutantStatus::Survived).count();
    let unknown = rows.len() - killed - survived;
    if unknown > 0 {
        notes.push(format!(
            "{unknown} mutant(s) could not be decided within the bounds — raise depth or path budget to classify them"
        ));
    }
    SpecCoverage {
        base_verdict: base.verdict,
        killed,
        survived,
        unknown,
        score: (!rows.is_empty())
            .then(|| (killed as f64 / rows.len() as f64 * 1000.0).round() / 10.0),
        total: rows.len(),
        mutants: rows,
        notes,
    }
}
