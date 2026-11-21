//! Runtime verification: incremental monitoring of LTLf formulas over a
//! stream of states.
//!
//! Instead of evaluating a whole [`Trace`][crate::ltl::Trace] after the fact, a [`Monitor`]
//! consumes states one at a time (a test, a request log, a CI run) and
//! reports the verdict the moment it becomes inevitable. This is the
//! classic formula-progression construction: each `feed` rewrites the
//! formula into the *residual obligation* for the remaining suffix.
//!
//! Boundary convention (LTLf): when the stream closes, synthesized `X`
//! obligations (from unrolling `G`/`F`/`U`/`W`/`R`) evaluate weakly — `G p`
//! is satisfied at the end, `F p` is not. A user's own `X` unwraps during
//! progression, so `X a` on a one-state log correctly reports violated.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};

use crate::ltl::{Formula, State};

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

/// The residual obligation after consuming one state: `w ⊨ f` at this
/// position iff `w[1..] ⊨ progress(f, w[0])`.
fn progress(f: &Formula, s: &State) -> Formula {
    match f {
        Formula::True => Formula::True,
        Formula::False => Formula::False,
        Formula::Atom(name) => {
            if s.contains(name) { Formula::True } else { Formula::False }
        }
        Formula::Not(arg) => simp_not(progress(arg, s)),
        Formula::And(l, r) => simp_and(progress(l, s), progress(r, s)),
        Formula::Or(l, r) => simp_or(progress(l, s), progress(r, s)),
        // l -> r  ≡  !l || r
        Formula::Implies(l, r) => {
            simp_or(simp_not(progress(l, s)), progress(r, s))
        }
        // l <-> r  ≡  (l && r) || (!l && !r)
        Formula::Iff(l, r) => {
            let (pl, pr) = (progress(l, s), progress(r, s));
            simp_or(
                simp_and(pl.clone(), pr.clone()),
                simp_and(simp_not(pl), simp_not(pr)),
            )
        }
        Formula::Next(arg) => (**arg).clone(),
        // The residual for the remaining suffix is the temporal formula
        // itself — `aw ⊨ G f` iff `aw ⊨ f && w ⊨ G f`. No `X` wrapper: a
        // wrapper would defer the obligation one position too far.
        Formula::Globally(arg) => {
            simp_and(progress(arg, s), (**arg).clone().into_globally())
        }
        Formula::Finally(arg) => {
            simp_or(progress(arg, s), (**arg).clone().into_finally())
        }
        // l U r  ≡  r || (l && X (l U r)); the X part is the suffix
        // obligation `l U r` itself.
        Formula::Until(l, r) => simp_or(
            progress(r, s),
            simp_and(progress(l, s), Formula::Until(l.clone(), r.clone())),
        ),
        Formula::Weak(l, r) => simp_or(
            progress(r, s),
            simp_and(progress(l, s), Formula::Weak(l.clone(), r.clone())),
        ),
        // l R r  ≡  r && (l || X (l R r))
        Formula::Release(l, r) => simp_and(
            progress(r, s),
            simp_or(progress(l, s), Formula::Release(l.clone(), r.clone())),
        ),
    }
}

impl Formula {
    fn into_globally(self) -> Formula {
        Formula::Globally(Box::new(self))
    }
    fn into_finally(self) -> Formula {
        Formula::Finally(Box::new(self))
    }
}

/// Verdict on the residual when the stream ends — plain LTLf evaluation on
/// the empty suffix. `G`/`W`/`R` are vacuously satisfied there, `F`/`U` and
/// atoms are not; a user's `X` is unwrapped by progression before the
/// boundary is ever reached.
fn eval_end(f: &Formula) -> bool {
    crate::ltl::eval_at(f, &Vec::new(), 0)
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
}

impl Monitor {
    /// A monitor for `formula`, before any state has been fed.
    pub fn new(formula: &Formula) -> Self {
        Monitor {
            residual: formula.clone(),
            decided: None,
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
        if self.decided.is_none() {
            self.residual = progress(&self.residual, state);
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
