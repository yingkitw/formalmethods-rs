//! Runtime verification: incremental monitoring of LTLf formulas over a
//! stream of states.
//!
//! Instead of evaluating a whole [`Trace`][crate::ltl::Trace] after the fact, a [`Monitor`]
//! consumes states one at a time (a test, a request log, a CI run) and
//! reports the verdict the moment it becomes inevitable. This is the
//! classic formula-progression construction: each `feed` rewrites the
//! formula into the *residual obligation* for the remaining suffix.
//!
//! Two subtleties:
//!
//! - **The `X` anchor.** `X φ` consumed at a position becomes
//!   `φ ∧ F true`: the obligation is `φ` *and* that a next position must
//!   exist. Without the anchor, `X !a` on a one-state log would wrongly
//!   close as satisfied (`!a` is vacuously true on the empty suffix).
//! - **Past operators need no derivative.** `Y`/`O`/`H`/`S` evaluate
//!   immediately against the consumed prefix — history is concrete, so
//!   they fold to literals instead of being carried forward.
//!
//! Boundary convention (LTLf): when the stream closes, the residual is
//! evaluated on the empty suffix — `G`/`W`/`R` vacuously pass, `F`/`U`
//! and atoms fail.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, string::String, vec::Vec};

use crate::ltl::{Formula, State, Trace, eval_at, print_ltl};

/// What the monitor knows so far.
///
/// - [`Satisfied`](MonitorVerdict::Satisfied): the property is decided —
///   no suffix can violate it (e.g. `F p` already saw `p`).
/// - [`Violated`](MonitorVerdict::Violated): decided — no suffix can save it.
/// - [`Pending`](MonitorVerdict::Pending): undecided; keep feeding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorVerdict {
    Satisfied,
    Violated,
    Pending,
}

impl MonitorVerdict {
    /// The wire name the TS engine uses for the same notion.
    pub fn as_str(&self) -> &'static str {
        match self {
            MonitorVerdict::Satisfied => "met",
            MonitorVerdict::Violated => "broken",
            MonitorVerdict::Pending => "owed",
        }
    }
}

impl serde::Serialize for MonitorVerdict {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Progression (Bauer's residual construction, adapted to LTLf)
// ---------------------------------------------------------------------------

fn simp_not(f: Formula) -> Formula {
    match f {
        Formula::True => Formula::False,
        Formula::False => Formula::True,
        Formula::Not(inner) => *inner,
        _ => Formula::Not(Box::new(f)),
    }
}

fn simp_and(l: Formula, r: Formula) -> Formula {
    match (l, r) {
        (Formula::False, _) | (_, Formula::False) => Formula::False,
        (Formula::True, r) => r,
        (l, Formula::True) => l,
        (l, r) if l == r => l,
        (l, r) => Formula::And(Box::new(l), Box::new(r)),
    }
}

fn simp_or(l: Formula, r: Formula) -> Formula {
    match (l, r) {
        (Formula::True, _) | (_, Formula::True) => Formula::True,
        (Formula::False, r) => r,
        (l, Formula::False) => l,
        (l, r) if l == r => l,
        (l, r) => Formula::Or(Box::new(l), Box::new(r)),
    }
}

/// `F true` — "at least one position remains". Conjuncted into `X`
/// residuals so the boundary case (stream ends) fails correctly.
fn exists_next() -> Formula {
    Formula::Finally(Box::new(Formula::True))
}

/// The residual obligation after consuming position `i` of `trace`:
/// `t[i..] ⊨ f` iff `t[i+1..] ⊨ progress(f, i, t)`.
fn progress(f: &Formula, i: usize, trace: &Trace) -> Formula {
    match f {
        Formula::True => Formula::True,
        Formula::False => Formula::False,
        Formula::Atom(name) => {
            if trace[i].contains(name) { Formula::True } else { Formula::False }
        }
        Formula::Not(arg) => simp_not(progress(arg, i, trace)),
        Formula::And(l, r) => simp_and(progress(l, i, trace), progress(r, i, trace)),
        Formula::Or(l, r) => simp_or(progress(l, i, trace), progress(r, i, trace)),
        // l -> r  ≡  !l || r
        Formula::Implies(l, r) => {
            simp_or(simp_not(progress(l, i, trace)), progress(r, i, trace))
        }
        // l <-> r  ≡  (l && r) || (!l && !r)
        Formula::Iff(l, r) => {
            let (pl, pr) = (progress(l, i, trace), progress(r, i, trace));
            simp_or(
                simp_and(pl.clone(), pr.clone()),
                simp_and(simp_not(pl), simp_not(pr)),
            )
        }
        // `aw ⊨ X f` iff `w ⊨ f` *and w is non-empty* — the `F true`
        // conjunct carries the position-existence anchor to the boundary.
        Formula::Next(arg) => simp_and((**arg).clone(), exists_next()),
        // The residual for the remaining suffix is the temporal formula
        // itself — `aw ⊨ G f` iff `aw ⊨ f && w ⊨ G f`. No `X` wrapper: a
        // wrapper would defer the obligation one position too far.
        Formula::Globally(arg) => {
            simp_and(progress(arg, i, trace), Formula::Globally(arg.clone()))
        }
        Formula::Finally(arg) => {
            simp_or(progress(arg, i, trace), Formula::Finally(arg.clone()))
        }
        // l U r  ≡  r || (l && X (l U r)); the X part is the suffix
        // obligation `l U r` itself.
        Formula::Until(l, r) => simp_or(
            progress(r, i, trace),
            simp_and(progress(l, i, trace), Formula::Until(l.clone(), r.clone())),
        ),
        Formula::Weak(l, r) => simp_or(
            progress(r, i, trace),
            simp_and(progress(l, i, trace), Formula::Weak(l.clone(), r.clone())),
        ),
        // l R r  ≡  r && (l || X (l R r))
        Formula::Release(l, r) => simp_and(
            progress(r, i, trace),
            simp_or(progress(l, i, trace), Formula::Release(l.clone(), r.clone())),
        ),
        // History is concrete: past operators resolve now, to a literal.
        Formula::Yesterday(..)
        | Formula::Once(..)
        | Formula::Historically(..)
        | Formula::Since(..) => {
            if eval_at(f, trace, i) { Formula::True } else { Formula::False }
        }
    }
}

/// Fold `true`/`false` literals through every connective of a residual —
/// `G true` is `true`, `l W false` is `G l`, and so on.
fn simplify(f: Formula) -> Formula {
    match f {
        Formula::Not(arg) => {
            let a = simplify(*arg);
            match a {
                Formula::True => Formula::False,
                Formula::False => Formula::True,
                a => Formula::Not(Box::new(a)),
            }
        }
        Formula::And(..)
        | Formula::Or(..)
        | Formula::Implies(..)
        | Formula::Iff(..)
        | Formula::Until(..)
        | Formula::Weak(..)
        | Formula::Release(..)
        | Formula::Since(..) => {
            let (kind_l, kind_r) = match &f {
                Formula::And(l, r)
                | Formula::Or(l, r)
                | Formula::Implies(l, r)
                | Formula::Iff(l, r)
                | Formula::Until(l, r)
                | Formula::Weak(l, r)
                | Formula::Release(l, r)
                | Formula::Since(l, r) => (l.clone(), r.clone()),
                _ => unreachable!(),
            };
            let (l, r) = (simplify(*kind_l), simplify(*kind_r));
            let (lt, lf, rt, rf) = (
                l == Formula::True,
                l == Formula::False,
                r == Formula::True,
                r == Formula::False,
            );
            match f {
                Formula::And(..) => {
                    if lf || rf {
                        Formula::False
                    } else if lt {
                        r
                    } else if rt {
                        l
                    } else {
                        Formula::And(Box::new(l), Box::new(r))
                    }
                }
                Formula::Or(..) => {
                    if lt || rt {
                        Formula::True
                    } else if lf {
                        r
                    } else if rf {
                        l
                    } else {
                        Formula::Or(Box::new(l), Box::new(r))
                    }
                }
                Formula::Implies(..) => {
                    if lf || rt {
                        Formula::True
                    } else if rf {
                        simplify(Formula::Not(Box::new(l)))
                    } else if lt {
                        r
                    } else {
                        Formula::Implies(Box::new(l), Box::new(r))
                    }
                }
                Formula::Iff(..) => {
                    if lt {
                        r
                    } else if rt {
                        l
                    } else if lf {
                        simplify(Formula::Not(Box::new(r)))
                    } else if rf {
                        simplify(Formula::Not(Box::new(l)))
                    } else {
                        Formula::Iff(Box::new(l), Box::new(r))
                    }
                }
                Formula::Until(..) => {
                    if rt {
                        Formula::True
                    } else if rf {
                        Formula::False
                    } else if lf {
                        r
                    } else {
                        Formula::Until(Box::new(l), Box::new(r))
                    }
                }
                Formula::Weak(..) => {
                    if rt {
                        Formula::True
                    } else if lf {
                        r
                    } else if rf {
                        Formula::Globally(Box::new(l))
                    } else {
                        Formula::Weak(Box::new(l), Box::new(r))
                    }
                }
                Formula::Release(..) => {
                    if rf {
                        Formula::False
                    } else if rt {
                        Formula::True
                    } else if lt {
                        r
                    } else {
                        Formula::Release(Box::new(l), Box::new(r))
                    }
                }
                Formula::Since(..) => Formula::Since(Box::new(l), Box::new(r)),
                _ => unreachable!(),
            }
        }
        Formula::Globally(arg) => {
            let a = simplify(*arg);
            match a {
                Formula::True => Formula::True,
                Formula::False => Formula::False,
                a => Formula::Globally(Box::new(a)),
            }
        }
        Formula::Finally(arg) => {
            let a = simplify(*arg);
            match a {
                // `F true` is NOT folded: it is the `X` anchor — true at any
                // real position but false on the empty suffix, exactly the
                // "a next position must exist" obligation.
                Formula::False => Formula::False,
                a => Formula::Finally(Box::new(a)),
            }
        }
        // true/false/atom/next/past ops: literal leaves, nothing to fold.
        _ => f,
    }
}

/// Verdict on the residual when the stream ends — plain LTLf evaluation on
/// the empty suffix. `G`/`W`/`R` are vacuously satisfied there, `F`/`U`
/// and atoms are not; `X` obligations carry their `F true` anchor.
fn eval_end(f: &Formula) -> bool {
    eval_at(f, &Vec::new(), 0)
}

// ---------------------------------------------------------------------------
// Monitor
// ---------------------------------------------------------------------------

/// An incremental LTLf monitor over a stream of states.
///
/// ```
/// use formalmethods::{parse_ltl, monitor::Monitor, trace};
///
/// let mut m = Monitor::new(&parse_ltl("G (request -> F response)").unwrap());
/// for state in trace(&[&["request"], &["response"]]) {
///     m.feed(&state);
/// }
/// assert!(m.close());
/// ```
#[derive(Debug, Clone)]
pub struct Monitor {
    residual: Formula,
    decided: Option<bool>,
    /// Consumed prefix — history for past-operator resolution.
    history: Trace,
}

impl Monitor {
    /// A monitor for `formula`, before any state has been fed.
    pub fn new(formula: &Formula) -> Self {
        Monitor {
            residual: formula.clone(),
            decided: None,
            history: Vec::new(),
        }
    }

    /// The obligation the remaining suffix must satisfy.
    pub fn residual(&self) -> &Formula {
        &self.residual
    }

    /// `true` once the verdict is decided (`Satisfied`/`Violated`), `false`
    /// while `Pending`.
    pub fn is_decided(&self) -> bool {
        self.decided.is_some()
    }

    /// The current verdict without closing the stream.
    pub fn verdict(&self) -> MonitorVerdict {
        match self.decided {
            Some(true) => MonitorVerdict::Satisfied,
            Some(false) => MonitorVerdict::Violated,
            None => MonitorVerdict::Pending,
        }
    }

    /// Consume one state; returns the (possibly still `Pending`) verdict.
    pub fn feed(&mut self, state: &State) -> MonitorVerdict {
        self.history.push(state.clone());
        if self.decided.is_none() {
            let i = self.history.len() - 1;
            self.residual = simplify(progress(&self.residual, i, &self.history));
            match self.residual {
                Formula::True => self.decided = Some(true),
                Formula::False => self.decided = Some(false),
                _ => {}
            }
        }
        self.verdict()
    }

    /// Close the stream and return the final verdict: `true` if the formula
    /// holds over the states fed so far.
    ///
    /// Agrees with `satisfies_trace(&formula, &consumed_trace)`.
    pub fn close(&self) -> bool {
        self.decided.unwrap_or_else(|| eval_end(&self.residual))
    }
}

// ---------------------------------------------------------------------------
// Batch view — the same progression rendered as a step table
// ---------------------------------------------------------------------------

/// One monitored step: the state consumed, the residual obligation left,
/// and whether the property is met, broken, or still owed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MonitorStep {
    pub position: usize,
    pub atoms: State,
    /// The obligation still owed *after* consuming this state, printed.
    pub residual: String,
    pub status: MonitorVerdict,
}

/// The monitor's story over a whole finite trace — same engine as
/// [`Monitor::feed`], rendered row by row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MonitorRun {
    pub formula: String,
    pub steps: Vec<MonitorStep>,
    /// Whether the whole trace satisfies the formula (same answer as
    /// `satisfies_trace`).
    pub verdict: bool,
}

/// Run the monitor over a finite trace: one residual-obligation row per
/// state. Step statuses are decided the moment the residual folds to a
/// literal; a non-literal residual at the last position closes under the
/// end-of-trace rule (`X` fails, `G`/`W`/`R` vacuously pass, `F`/`U` fail).
pub fn monitor_trace(f: &Formula, trace: &Trace) -> MonitorRun {
    let mut residual = f.clone();
    let steps = trace
        .iter()
        .enumerate()
        .map(|(i, atoms)| {
            residual = simplify(progress(&residual, i, trace));
            let status = match residual {
                Formula::True => MonitorVerdict::Satisfied,
                Formula::False => MonitorVerdict::Violated,
                _ if i == trace.len() - 1 => {
                    if eval_end(&residual) {
                        MonitorVerdict::Satisfied
                    } else {
                        MonitorVerdict::Violated
                    }
                }
                _ => MonitorVerdict::Pending,
            };
            MonitorStep {
                position: i,
                atoms: atoms.clone(),
                residual: print_ltl(&residual),
                status,
            }
        })
        .collect::<Vec<_>>();
    MonitorRun {
        formula: print_ltl(f),
        verdict: if trace.is_empty() {
            eval_at(f, trace, 0)
        } else {
            steps.last().is_some_and(|s| s.status == MonitorVerdict::Satisfied)
        },
        steps,
    }
}
