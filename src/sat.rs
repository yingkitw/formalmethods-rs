//! SAT-backed bounded model checking (varisat) — a symbolic alternative to
//! `model_check`'s explicit lasso enumeration, aimed at bigger models where
//! DFS enumeration slows down.
//!
//! Encoding: one-hot positions — `x[i][s]` reads "lasso position `i` occupies
//! state `s`". A lasso is positions `0..=k` with a back-edge `k → l` (dead
//! ends stutter: a state with no declared successors self-loops). The formula
//! is evaluated symbolically along the periodic path: temporal operators
//! expand over the same decisive window `eval_periodic_at` uses
//! (`i..max(i,l)+cycle_len`), so `U`/`W`/`R`/`F`/`G` become flat
//! disjunctions of conjunctions — no cyclic definitions, no fixed-point
//! gadgets needed.
//!
//! One CNF per (length, loop-start) pair, solved at increasing length: the
//! first satisfiable instance is the shortest counterexample this encoding
//! sees. Verdict honesty mirrors `model_check`: `holds` only when depth
//! reaches `|states|` — the engine's documented completeness convention
//! (every *simple* lasso fits in `|states|` positions); anything shallower
//! reports `incomplete`, said out loud.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use varisat::{ExtendFormula, Lit, Solver};

use crate::ltl::{Formula, Trace, eval_periodic_at, periodic_truth_table};
use crate::model::{
    CounterexampleWindow, KripkeSpec, LassoCounterexample, Verdict,
};

/// Tuning knobs for [`sat_check`].
#[derive(Debug, Clone, Default)]
pub struct SatOptions {
    /// Max lasso length in positions. Default: `|states|` — the
    /// completeness convention shared with `model_check`.
    pub depth: Option<usize>,
}

/// The outcome of [`sat_check`].
#[derive(Debug, Clone, Serialize)]
pub struct SatResult {
    pub verdict: Verdict,
    /// Positions actually searched (the bound used).
    pub depth: usize,
    pub state_count: usize,
    /// (length, loop-start) instances built and solved.
    pub checked_lassos: usize,
    /// The violating lasso, when the verdict is `violated`.
    pub counterexample: Option<LassoCounterexample>,
    pub notes: Vec<String>,
}

/// One-hot bounded model checker for a fixed lasso shape `(k, l)`.
struct Enc<'a> {
    solver: Solver<'a>,
    /// `x[i][s]` — position `i` is state index `s`.
    x: Vec<Vec<Lit>>,
    /// Memoized Tseitin literals, keyed `(position, formula-node id)`.
    memo: HashMap<(usize, usize), Lit>,
    /// A literal pinned true once, reused for `true`/`false` constants.
    top: Lit,
    /// Lasso shape: last index `k`, loop start `l`, state count `n`.
    k: usize,
    l: usize,
    /// `labeled[a][i]` — state index `i` is labeled with atom `a`.
    labeled: HashMap<String, Vec<usize>>,
    /// Successor table with dead-end stuttering folded in.
    succ: Vec<Vec<usize>>,
}

impl<'a> Enc<'a> {
    fn new(spec: &'a KripkeSpec, k: usize, l: usize) -> Self {
        let mut solver = Solver::new();
        let x: Vec<Vec<Lit>> = (0..=k)
            .map(|_| (0..spec.states.len()).map(|_| solver.new_lit()).collect())
            .collect();
        let top = solver.new_lit();
        solver.add_clause(&[top]);

        let idx: BTreeMap<&str, usize> = spec
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let labeled: HashMap<String, Vec<usize>> = {
            let mut m: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, s) in spec.states.iter().enumerate() {
                for a in &s.atoms {
                    m.entry(a.clone()).or_default().push(i);
                }
            }
            m
        };
        let succ: Vec<Vec<usize>> = spec
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let declared: Vec<usize> =
                    s.next.iter().filter_map(|t| idx.get(t.as_str()).copied()).collect();
                // Dead ends stutter — the same convention `Checker::walk` uses.
                if declared.is_empty() { vec![i] } else { declared }
            })
            .collect();

        let mut enc = Enc {
            solver,
            x,
            memo: HashMap::new(),
            top,
            k,
            l,
            labeled,
            succ,
        };
        enc.encode_skeleton(spec, &idx);
        enc
    }

    /// Path structure: exactly-one per position, initial states, transitions
    /// (including the `k → l` back-edge).
    fn encode_skeleton(&mut self, spec: &KripkeSpec, idx: &BTreeMap<&str, usize>) {
        for i in 0..=self.k {
            let row = &self.x[i];
            let at_least: Vec<Lit> = row.clone();
            self.solver.add_clause(&at_least);
            for a in 0..row.len() {
                for b in (a + 1)..row.len() {
                    self.solver.add_clause(&[!row[a], !row[b]]);
                }
            }
        }
        // Initial position must be an initial state.
        let init: Vec<Lit> = spec
            .initial
            .iter()
            .filter_map(|id| idx.get(id.as_str()))
            .map(|&s| self.x[0][s])
            .collect();
        if init.is_empty() {
            // No initial states — no lasso exists at all.
            self.solver.add_clause(&[]);
        } else {
            self.solver.add_clause(&init);
        }
        // Transitions: position i's successor is i+1, or l for i == k.
        for i in 0..=self.k {
            let next = if i < self.k { i + 1 } else { self.l };
            for (s, row) in self.x[i].iter().enumerate() {
                let mut clause = vec![!*row];
                clause.extend(self.succ[s].iter().map(|&t| self.x[next][t]));
                self.solver.add_clause(&clause);
            }
        }
    }

    fn and_lit(&mut self, lits: Vec<Lit>) -> Lit {
        match lits.len() {
            0 => self.top,
            1 => lits[0],
            _ => {
                let v = self.solver.new_lit();
                for &c in &lits {
                    self.solver.add_clause(&[!v, c]);
                }
                let mut clause: Vec<Lit> = lits.iter().map(|l| !*l).collect();
                clause.push(v);
                self.solver.add_clause(&clause);
                v
            }
        }
    }

    fn or_lit(&mut self, lits: Vec<Lit>) -> Lit {
        match lits.len() {
            0 => !self.top,
            1 => lits[0],
            _ => {
                let v = self.solver.new_lit();
                let mut clause = vec![!v];
                clause.extend(lits.iter());
                self.solver.add_clause(&clause);
                for &c in &lits {
                    self.solver.add_clause(&[!c, v]);
                }
                v
            }
        }
    }

    /// The periodic-path positions scanned by a future operator at `i`
    /// (`eval_periodic_at`'s decisive window): `i..=k` then one wrap
    /// `l..i` when `i` sits inside the cycle.
    fn seq(&self, i: usize) -> Vec<usize> {
        (i..=self.k).chain(self.l..i).collect()
    }

    /// The distinct positions a G/F window covers from `i`: everything from
    /// `min(i, l)` onward.
    fn window(&self, i: usize) -> Vec<usize> {
        (i.min(self.l)..=self.k).collect()
    }

    /// `lit(i, f)` — a literal true iff `f` holds at position `i` of the
    /// periodic lasso, encoded by structural recursion (Tseitin).
    fn lit(&mut self, i: usize, f: &Formula) -> Lit {
        let key = (i, f as *const Formula as usize);
        if let Some(&v) = self.memo.get(&key) {
            return v;
        }
        let v = match f {
            Formula::True => self.top,
            Formula::False => !self.top,
            Formula::Atom(a) => {
                let states = self.labeled.get(a).cloned().unwrap_or_default();
                self.or_lit(states.iter().map(|&s| self.x[i][s]).collect())
            }
            Formula::Not(arg) => !self.lit(i, arg),
            Formula::And(l, r) => {
                let (a, b) = (self.lit(i, l), self.lit(i, r));
                self.and_lit(vec![a, b])
            }
            Formula::Or(l, r) => {
                let (a, b) = (self.lit(i, l), self.lit(i, r));
                self.or_lit(vec![a, b])
            }
            Formula::Implies(l, r) => {
                let (a, b) = (!self.lit(i, l), self.lit(i, r));
                self.or_lit(vec![a, b])
            }
            Formula::Iff(l, r) => {
                let (a, b) = (self.lit(i, l), self.lit(i, r));
                let v = self.solver.new_lit();
                // v ↔ (a ↔ b)
                self.solver.add_clause(&[!v, !a, b]);
                self.solver.add_clause(&[!v, a, !b]);
                self.solver.add_clause(&[v, !a, !b]);
                self.solver.add_clause(&[v, a, b]);
                v
            }
            // Next on the periodic path: i+1, wrapping k → l.
            Formula::Next(arg) => {
                let next = if i < self.k { i + 1 } else { self.l };
                self.lit(next, arg)
            }
            Formula::Globally(arg) => {
                let lits: Vec<Lit> = self.window(i).iter().map(|&t| self.lit(t, arg)).collect();
                self.and_lit(lits)
            }
            Formula::Finally(arg) => {
                let lits: Vec<Lit> = self.window(i).iter().map(|&t| self.lit(t, arg)).collect();
                self.or_lit(lits)
            }
            // Past operators see concrete history 0..=i — no wrapping.
            Formula::Yesterday(arg) => {
                if i == 0 {
                    !self.top
                } else {
                    self.lit(i - 1, arg)
                }
            }
            Formula::Once(arg) => {
                let lits: Vec<Lit> = (0..=i).map(|t| self.lit(t, arg)).collect();
                self.or_lit(lits)
            }
            Formula::Historically(arg) => {
                let lits: Vec<Lit> = (0..=i).map(|t| self.lit(t, arg)).collect();
                self.and_lit(lits)
            }
            Formula::Since(l, r) => {
                let mut terms = Vec::with_capacity(i + 1);
                for j in 0..=i {
                    let mut conj = vec![self.lit(j, r)];
                    for t in (j + 1)..=i {
                        conj.push(self.lit(t, l));
                    }
                    terms.push(self.and_lit(conj));
                }
                self.or_lit(terms)
            }
            Formula::Until(l, r) => self.until_lit(i, l, r),
            Formula::Weak(l, r) => {
                let u = self.until_lit(i, l, r);
                let g = {
                    let lits: Vec<Lit> =
                        self.window(i).iter().map(|&t| self.lit(t, l)).collect();
                    self.and_lit(lits)
                };
                self.or_lit(vec![u, g])
            }
            // φ R ψ ≡ ¬(¬φ U ¬ψ)
            Formula::Release(l, r) => {
                let nl = |e: &mut Self, t| !e.lit(t, l);
                let nr = |e: &mut Self, t| !e.lit(t, r);
                !self.until_with(i, nl, nr)
            }
        };
        self.memo.insert(key, v);
        v
    }

    fn until_lit(&mut self, i: usize, l: &Formula, r: &Formula) -> Lit {
        self.until_with(i, |e, t| e.lit(t, l), |e, t| e.lit(t, r))
    }

    /// `∨_{p ∈ seq(i)} ( rf(p) ∧ ∧_{t before p} lf(t) )` — the flat U
    /// expansion over the decisive window.
    fn until_with(
        &mut self,
        i: usize,
        mut lf: impl FnMut(&mut Self, usize) -> Lit,
        mut rf: impl FnMut(&mut Self, usize) -> Lit,
    ) -> Lit {
        let seq = self.seq(i);
        let mut terms = Vec::with_capacity(seq.len());
        for (idx, &p) in seq.iter().enumerate() {
            let mut conj = vec![rf(self, p)];
            for &t in &seq[..idx] {
                conj.push(lf(self, t));
            }
            terms.push(self.and_lit(conj));
        }
        self.or_lit(terms)
    }

    /// One bounded instance: SAT iff a violating lasso of this shape exists.
    fn violates(&mut self, formula: &Formula) -> Option<Vec<usize>> {
        let root = self.lit(0, formula);
        self.solver.add_clause(&[!root]);
        match self.solver.solve() {
            Ok(true) => {
                let model = self.solver.model()?;
                let on: BTreeSet<usize> = model
                    .iter()
                    .filter(|l| l.is_positive())
                    .map(|l| l.var().index())
                    .collect();
                Some(
                    self.x
                        .iter()
                        .map(|row| {
                            row.iter()
                                .position(|l| on.contains(&l.var().index()))
                                .unwrap_or(0)
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }
}

/// Check an LTL formula against a Kripke structure under infinite-run
/// semantics, using a SAT solver instead of explicit enumeration.
///
/// Every `(length, loop-start)` pair up to `depth` becomes a one-hot CNF;
/// the first satisfiable one yields a counterexample (replayed through
/// `eval_periodic_at` before it is reported — the SAT encoding is verified,
/// never trusted). `holds` requires `depth ≥ |states|`, the completeness
/// convention `model_check` documents; otherwise `incomplete`.
///
/// The spec is expected to be valid (see `validate_kripke`); unknown state
/// ids in `next` are ignored exactly like the explicit checker does.
pub fn sat_check(formula: &Formula, spec: &KripkeSpec, opts: &SatOptions) -> SatResult {
    let n = spec.states.len();
    let depth = opts.depth.unwrap_or(n).clamp(1, n.max(1));
    let mut checked = 0usize;

    for len in 1..=depth {
        for l in 0..len {
            checked += 1;
            let mut enc = Enc::new(spec, len - 1, l);
            if let Some(path_idx) = enc.violates(formula) {
                let path: Vec<String> = path_idx
                    .iter()
                    .map(|&s| spec.states[s].id.clone())
                    .collect();
                return SatResult {
                    verdict: Verdict::Violated,
                    depth,
                    state_count: n,
                    checked_lassos: checked,
                    counterexample: Some(counterexample(formula, spec, &path, l)),
                    notes: vec![format!(
                        "SAT-found lasso of {len} positions (loop start {l}); \
                         verified by eval_periodic_at"
                    )],
                };
            }
        }
    }

    let complete = depth >= n;
    SatResult {
        verdict: if complete { Verdict::Holds } else { Verdict::Incomplete },
        depth,
        state_count: n,
        checked_lassos: checked,
        counterexample: None,
        notes: if complete {
            vec![]
        } else {
            vec![format!(
                "depth {depth} < |states| {n} — longer lassos remain unexplored"
            )]
        },
    }
}

/// Rebuild the found lasso into the standard counterexample shape — and
/// re-verify it against the real evaluator before reporting.
fn counterexample(
    formula: &Formula,
    spec: &KripkeSpec,
    path: &[String],
    cycle_start: usize,
) -> LassoCounterexample {
    let atoms_of = |id: &str| {
        spec.states
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.atoms.clone())
            .unwrap_or_default()
    };
    let pt = crate::ltl::PeriodicTrace {
        prefix: path[..cycle_start].iter().map(|s| atoms_of(s)).collect(),
        cycle: path[cycle_start..].iter().map(|s| atoms_of(s)).collect(),
    };
    debug_assert!(
        !eval_periodic_at(formula, &pt, 0),
        "SAT counterexample must violate the formula"
    );
    let cycle_len = path.len() - cycle_start;
    let reps = (12usize.div_ceil(cycle_len)).max(2);
    let mut ids: Vec<String> = path[..cycle_start].to_vec();
    let mut atoms: Trace = pt.prefix.clone();
    for _ in 0..reps {
        ids.extend(path[cycle_start..].iter().cloned());
        atoms.extend(pt.cycle.iter().cloned());
    }
    LassoCounterexample {
        path: path.to_vec(),
        cycle_start,
        window: CounterexampleWindow {
            ids,
            atoms,
            cycle_start,
            table: periodic_truth_table(formula, &pt, (pt.prefix.len() + cycle_len * 2).max(1)),
        },
    }
}
