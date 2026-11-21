//! Kripke structures, lasso-based model checking, and coverage analysis.
//!
//! The model checker enumerates every *simple path* of the structure up to a
//! depth bound, and for each path every *lasso* (a back-edge closing the path
//! into a cycle, plus a stuttering interpretation of dead ends).  LTL formulas
//! are evaluated on each lasso under infinite semantics.
//!
//! Soundness:  a reported counterexample is always a genuine violation.
//! Completeness: with depth = |states| and no path budget hit, `holds` is a
//! genuine exhaustive verdict — every LTL violation of a finite Kripke
//! structure is witnessed by some simple-prefix lasso.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::ltl::{
    Formula, PeriodicTrace, State, Trace, TruthTable, eval_periodic_at, periodic_truth_table,
};

/// One state of a [`KripkeSpec`]: an id, the atoms true there, and the ids of
/// its successors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KripkeState {
    pub id: String,
    pub atoms: State,
    pub next: Vec<String>,
}

/// A Kripke structure: atoms, initial state ids, and labelled states with
/// transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KripkeSpec {
    pub atoms: BTreeSet<String>,
    pub initial: Vec<String>,
    pub states: Vec<KripkeState>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// An error produced while validating a Kripke structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KripkeError(pub String);

impl fmt::Display for KripkeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KripkeError {}

fn fail<T>(message: impl Into<String>) -> Result<T, KripkeError> {
    Err(KripkeError(message.into()))
}

fn as_string_array(v: Option<&serde_json::Value>, what: &str) -> Result<Vec<String>, KripkeError> {
    match v {
        Some(serde_json::Value::Array(items))
            if items.iter().all(|x| x.as_str().is_some()) =>
        {
            Ok(items
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect())
        }
        _ => fail(format!("{what} must be an array of strings")),
    }
}

/// Validate a Kripke structure given as JSON, returning the checked model
/// plus non-fatal warnings.
///
/// - Every referenced state id (`initial`, `next`) must exist.
/// - Atoms used by a state but not declared in `atoms` are added, with a
///   warning for each.
/// - A model with no initial states earns a warning — every property holds
///   vacuously on an empty system.
pub fn parse_kripke(input: &serde_json::Value) -> Result<(KripkeSpec, Vec<String>), KripkeError> {
    let raw = input
        .as_object()
        .ok_or_else(|| KripkeError("model must be an object with `atoms`, `initial`, and `states`".into()))?;
    let mut warnings = Vec::new();

    let mut atoms: BTreeSet<String> = as_string_array(raw.get("atoms"), "model.atoms")?
        .into_iter()
        .collect();
    let initial = as_string_array(raw.get("initial"), "model.initial")?;
    let raw_states = match raw.get("states") {
        Some(serde_json::Value::Array(items)) if !items.is_empty() => items,
        _ => return fail("model.states must be a non-empty array"),
    };

    let mut states = Vec::with_capacity(raw_states.len());
    let mut ids = HashSet::new();
    for s in raw_states {
        let st = s
            .as_object()
            .ok_or_else(|| KripkeError("each state must be an object".into()))?;
        let id = match st.get("id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return fail("each state needs a string `id`"),
        };
        if !ids.insert(id.clone()) {
            return fail(format!("duplicate state id '{id}'"));
        }
        let state_atoms = as_string_array(st.get("atoms"), &format!("state '{id}'.atoms"))?;
        for a in &state_atoms {
            if atoms.insert(a.clone()) {
                warnings.push(format!(
                    "state '{id}' uses atom '{a}' which was not declared in model.atoms — added it for you"
                ));
            }
        }
        states.push(KripkeState {
            atoms: state_atoms.into_iter().collect(),
            next: as_string_array(st.get("next"), &format!("state '{id}'.next"))?,
            id,
        });
    }

    for id in &initial {
        if !ids.contains(id) {
            return fail(format!("initial state '{id}' does not exist"));
        }
    }
    for s in &states {
        for t in &s.next {
            if !ids.contains(t) {
                return fail(format!(
                    "state '{}' transitions to '{t}' which does not exist",
                    s.id
                ));
            }
        }
    }
    if initial.is_empty() {
        warnings.push(
            "model has no initial states — every property holds vacuously on an empty system"
                .to_string(),
        );
    }

    let mut deduped_initial: Vec<String> = Vec::new();
    for id in initial {
        if !deduped_initial.contains(&id) {
            deduped_initial.push(id);
        }
    }

    Ok((
        KripkeSpec {
            atoms,
            initial: deduped_initial,
            states,
        },
        warnings,
    ))
}

/// Validate a programmatically-built spec the same way [`parse_kripke`]
/// validates JSON: duplicate ids, dangling references, undeclared atoms
/// (auto-declared with warnings).
pub fn validate_kripke(model: &mut KripkeSpec) -> Result<Vec<String>, KripkeError> {
    if model.states.is_empty() {
        return fail("model.states must be a non-empty array");
    }
    let mut warnings = Vec::new();
    let mut ids = HashSet::new();
    for s in &mut model.states {
        if !ids.insert(s.id.clone()) {
            return fail(format!("duplicate state id '{}'", s.id));
        }
        let extra: Vec<String> = s
            .atoms
            .iter()
            .filter(|a| !model.atoms.contains(*a))
            .cloned()
            .collect();
        for a in extra {
            model.atoms.insert(a.clone());
            warnings.push(format!(
                "state '{}' uses atom '{a}' which was not declared in model.atoms — added it for you",
                s.id
            ));
        }
    }
    for id in &model.initial {
        if !ids.contains(id) {
            return fail(format!("initial state '{id}' does not exist"));
        }
    }
    for s in &model.states {
        for t in &s.next {
            if !ids.contains(t) {
                return fail(format!(
                    "state '{}' transitions to '{t}' which does not exist",
                    s.id
                ));
            }
        }
    }
    if model.initial.is_empty() {
        warnings.push(
            "model has no initial states — every property holds vacuously on an empty system"
                .to_string(),
        );
    }
    model.initial.dedup();
    Ok(warnings)
}

// ---------------------------------------------------------------------------
// Model checking
// ---------------------------------------------------------------------------

/// What the model checker proved — honestly labelled.
///
/// - [`Violated`](Verdict::Violated): at least one lasso breaks the property;
///   the counterexamples are genuine.
/// - [`Holds`](Verdict::Holds): the enumeration was exhaustive (full depth,
///   no truncation) and no lasso violates.
/// - [`Incomplete`](Verdict::Incomplete): the search was bounded (depth limit
///   or path budget) and nothing was found — *unknown*, said out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Violated,
    Holds,
    Incomplete,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Verdict::Violated => "violated",
            Verdict::Holds => "holds",
            Verdict::Incomplete => "incomplete",
        })
    }
}

/// A lasso-shaped counterexample: a simple prefix path that closes into a
/// cycle at `cycle_start`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LassoCounterexample {
    /// The simple prefix path; `path[cycle_start]` starts the loop.
    pub path: Vec<String>,
    pub cycle_start: usize,
    /// For display: ids and atoms along prefix + a few cycles of the loop.
    pub window: CounterexampleWindow,
}

/// The human-readable window around a counterexample: repeated cycle states
/// plus a truth table over the lasso.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CounterexampleWindow {
    pub ids: Vec<String>,
    pub atoms: Trace,
    pub cycle_start: usize,
    pub table: TruthTable,
}

/// The outcome of [`model_check`].
#[derive(Debug, Clone, Serialize)]
pub struct ModelCheckResult {
    pub verdict: Verdict,
    pub complete: bool,
    pub truncated: bool,
    pub depth: usize,
    pub state_count: usize,
    pub checked_lassos: usize,
    pub elapsed_ms: f64,
    pub counterexamples: Vec<LassoCounterexample>,
    pub notes: Vec<String>,
}

/// Tuning knobs for [`model_check`].
#[derive(Debug, Clone, Default)]
pub struct ModelCheckOptions {
    /// Max length of the (simple) prefix path to explore.
    /// Default: all of them (|states|).
    pub depth: Option<usize>,
    /// Cap on enumerated paths, to bound the state explosion. Default: 20000.
    pub path_budget: Option<usize>,
    /// Stop after this many counterexamples. Default: 3.
    pub max_counterexamples: Option<usize>,
}

fn to_periodic(model: &KripkeSpec, path: &[String], cycle_start: usize) -> PeriodicTrace {
    let atoms_of = |id: &str| -> State {
        model
            .states
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.atoms.clone())
            .unwrap_or_default()
    };
    PeriodicTrace {
        prefix: path[..cycle_start].iter().map(|s| atoms_of(s)).collect(),
        cycle: path[cycle_start..].iter().map(|s| atoms_of(s)).collect(),
    }
}

struct Checker<'a> {
    formula: &'a Formula,
    model: &'a KripkeSpec,
    state_by_id: HashMap<&'a str, &'a KripkeState>,
    depth: usize,
    budget: usize,
    max_ce: usize,
    paths: usize,
    truncated: bool,
    checked_lassos: usize,
    counterexamples: Vec<LassoCounterexample>,
}

impl<'a> Checker<'a> {
    fn new(formula: &'a Formula, model: &'a KripkeSpec, opts: &ModelCheckOptions) -> Self {
        let n = model.states.len();
        Checker {
            formula,
            model,
            state_by_id: model.states.iter().map(|s| (s.id.as_str(), s)).collect(),
            depth: opts.depth.unwrap_or(n).clamp(1, n.max(1)),
            budget: opts.path_budget.unwrap_or(20_000),
            max_ce: opts.max_counterexamples.unwrap_or(3),
            paths: 0,
            truncated: false,
            checked_lassos: 0,
            counterexamples: Vec::new(),
        }
    }

    fn done(&self) -> bool {
        self.truncated || self.counterexamples.len() >= self.max_ce
    }

    fn record(&mut self, path: &[String], cycle_start: usize) {
        let pt = to_periodic(self.model, path, cycle_start);
        self.checked_lassos += 1;
        if eval_periodic_at(self.formula, &pt, 0) {
            return;
        }
        let cycle_len = path.len() - cycle_start;
        let reps = (12usize.div_ceil(cycle_len)).max(2);
        let mut ids: Vec<String> = path[..cycle_start].to_vec();
        let mut atoms: Trace = pt.prefix.clone();
        for _ in 0..reps {
            ids.extend(path[cycle_start..].iter().cloned());
            atoms.extend(pt.cycle.iter().cloned());
        }
        self.counterexamples.push(LassoCounterexample {
            path: path.to_vec(),
            cycle_start,
            window: CounterexampleWindow {
                ids,
                atoms,
                cycle_start,
                table: periodic_truth_table(
                    self.formula,
                    &pt,
                    (pt.prefix.len() + cycle_len * 2).max(1),
                ),
            },
        });
    }

    /// Enumerate the lassos of one simple path, then recurse into fresh successors.
    fn walk(&mut self, path: &mut Vec<String>, visited: &mut HashSet<String>) {
        self.paths += 1;
        if self.paths > self.budget {
            self.truncated = true;
            return;
        }
        let last = path.last().unwrap();
        let st = self.state_by_id[last.as_str()];
        let nexts: Vec<&str> = st
            .next
            .iter()
            .map(|s| s.as_str())
            .filter(|t| self.state_by_id.contains_key(t))
            .collect();

        // Lassos closed by back-edges into the current path…
        for t in &nexts {
            if visited.contains(*t) {
                let cycle_start = path.iter().position(|p| p.as_str() == *t).unwrap();
                self.record(path, cycle_start);
                if self.counterexamples.len() >= self.max_ce {
                    return;
                }
            }
        }
        // …and the stutter of a dead end (the system freezes forever).
        if nexts.is_empty() {
            let cycle_start = path.len() - 1;
            self.record(path, cycle_start);
            if self.counterexamples.len() >= self.max_ce {
                return;
            }
        }

        if path.len() >= self.depth {
            return;
        }
        for t in nexts {
            if visited.contains(t) {
                continue;
            }
            visited.insert(t.to_string());
            path.push(t.to_string());
            self.walk(path, visited);
            path.pop();
            visited.remove(t);
            if self.done() {
                return;
            }
        }
    }
}

/// Check an LTL formula against a Kripke structure under infinite-run
/// semantics.
///
/// Enumerates every lasso (simple prefix + cycle, plus dead-end stuttering)
/// up to a depth bound. The verdict never overstates: `holds` only when the
/// enumeration was exhaustive.
pub fn model_check(
    formula: &Formula,
    model: &KripkeSpec,
    opts: &ModelCheckOptions,
) -> ModelCheckResult {
    let n = model.states.len();
    let started = Instant::now();
    let mut checker = Checker::new(formula, model, opts);

    for init in &model.initial {
        if checker.state_by_id.contains_key(init.as_str()) {
            checker.walk(&mut vec![init.clone()], &mut HashSet::from([init.clone()]));
        }
        if checker.done() {
            break;
        }
    }

    let mut notes = Vec::new();
    if checker.truncated {
        notes.push(format!(
            "path budget reached — only the first {} paths were explored",
            checker.budget
        ));
    }
    if checker.depth < n {
        notes.push(format!(
            "depth bound {} < {n} states — behaviors with longer prefixes were not explored",
            checker.depth
        ));
    }

    // With a full-depth, untruncated enumeration every infinite run of the
    // structure has been covered (up to repetition) — `holds` is exhaustive.
    let exhausted = !checker.truncated && checker.depth >= n;
    let verdict = if !checker.counterexamples.is_empty() {
        Verdict::Violated
    } else if exhausted {
        Verdict::Holds
    } else {
        Verdict::Incomplete
    };
    ModelCheckResult {
        verdict,
        complete: verdict != Verdict::Incomplete,
        truncated: checker.truncated,
        depth: checker.depth,
        state_count: n,
        checked_lassos: checker.checked_lassos,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        counterexamples: checker.counterexamples,
        notes,
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// State and transition coverage of a set of test traces against a model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoverageReport {
    pub total_states: usize,
    pub covered_states: Vec<String>,
    pub missing_states: Vec<String>,
    pub state_percent: f64,
    pub total_transitions: usize,
    /// Rendered as `"from -> to"`.
    pub covered_transitions: Vec<String>,
    /// Rendered as `"from -> to"`.
    pub missing_transitions: Vec<String>,
    pub transition_percent: f64,
    /// State ids appearing in traces but not in the model.
    pub unknown_states: Vec<String>,
}

/// Measure state and transition coverage of a set of test traces
/// (sequences of state ids).
pub fn analyze_coverage(model: &KripkeSpec, traces: &[Vec<String>]) -> CoverageReport {
    let state_ids: HashSet<&str> = model.states.iter().map(|s| s.id.as_str()).collect();
    let mut covered_states = BTreeSet::new();
    let mut covered_transitions: HashSet<(&str, &str)> = HashSet::new();
    let mut unknown = BTreeSet::new();

    for trace in traces {
        for id in trace {
            if state_ids.contains(id.as_str()) {
                covered_states.insert(id.clone());
            } else {
                unknown.insert(id.clone());
            }
        }
        for pair in trace.windows(2) {
            let (from, to) = (pair[0].as_str(), pair[1].as_str());
            if state_ids.contains(from) && state_ids.contains(to) {
                covered_transitions.insert((from, to));
            }
        }
    }

    let missing_states: Vec<String> = model
        .states
        .iter()
        .map(|s| s.id.clone())
        .filter(|id| !covered_states.contains(id))
        .collect();
    let all_transitions: Vec<(&str, &str)> = model
        .states
        .iter()
        .flat_map(|s| s.next.iter().map(move |t| (s.id.as_str(), t.as_str())))
        .collect();
    let covered: Vec<(&str, &str)> = all_transitions
        .iter()
        .copied()
        .filter(|&(f, t)| covered_transitions.contains(&(f, t)))
        .collect();
    let missing: Vec<(&str, &str)> = all_transitions
        .iter()
        .copied()
        .filter(|&(f, t)| !covered_transitions.contains(&(f, t)))
        .collect();

    let state_percent = if model.states.is_empty() {
        100.0
    } else {
        covered_states.len() as f64 / model.states.len() as f64 * 100.0
    };

    CoverageReport {
        total_states: model.states.len(),
        covered_states: covered_states.into_iter().collect(),
        missing_states,
        state_percent,
        total_transitions: all_transitions.len(),
        covered_transitions: covered.iter().map(|(f, t)| format!("{f} -> {t}")).collect(),
        missing_transitions: missing.iter().map(|(f, t)| format!("{f} -> {t}")).collect(),
        transition_percent: if all_transitions.is_empty() {
            100.0
        } else {
            covered.len() as f64 / all_transitions.len() as f64 * 100.0
        },
        unknown_states: unknown.into_iter().collect(),
    }
}
