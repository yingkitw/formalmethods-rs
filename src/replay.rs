//! Replay a model-check counterexample under finite semantics.
//!
//! A counterexample lasso is infinite; [`satisfies_trace`] (LTLf) judges
//! finite traces. [`falsifying_window`] unrolls the loop until the finite
//! trace still fails the property — a counterexample you can paste into a
//! regression test. Some violations genuinely need the loop (e.g. `F G p`
//! on a flicker that ends on `p`); those report `None` rather than a false
//! replay.

use serde::Serialize;

use crate::ltl::{Formula, PeriodicTrace, Trace, satisfies_trace};
use crate::model::{KripkeSpec, LassoCounterexample};

/// Rebuild the ultimately-periodic run a counterexample came from.
pub fn counterexample_run(model: &KripkeSpec, ce: &LassoCounterexample) -> PeriodicTrace {
    let atoms_of = |id: &str| {
        model
            .states
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.atoms.clone())
            .unwrap_or_default()
    };
    PeriodicTrace {
        prefix: ce.path[..ce.cycle_start].iter().map(|s| atoms_of(s)).collect(),
        cycle: ce.path[ce.cycle_start..].iter().map(|s| atoms_of(s)).collect(),
    }
}

/// A finite replay: the unrolled trace plus how many loop repetitions it
/// took to still falsify the property.
#[derive(Debug, Clone, Serialize)]
pub struct FalsifyingWindow {
    pub trace: Trace,
    pub reps: usize,
}

/// The shortest window (prefix + `reps` whole loops) that still falsifies
/// the property under LTLf — `None` when no tried unrolling falsifies,
/// meaning the violation witnesses infinity itself and is not finitely
/// replayable.
pub fn falsifying_window(
    f: &Formula,
    run: &PeriodicTrace,
    reps: &[usize],
) -> Option<FalsifyingWindow> {
    let reps: &[usize] = if reps.is_empty() { &[2, 3, 4, 6, 8] } else { reps };
    for &k in reps {
        let mut trace: Trace = run.prefix.clone();
        for _ in 0..k {
            trace.extend(run.cycle.iter().cloned());
        }
        if !satisfies_trace(f, &trace) {
            return Some(FalsifyingWindow { trace, reps: k });
        }
    }
    None
}
