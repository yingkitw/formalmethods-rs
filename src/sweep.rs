//! Bounded model checking made visible: re-check the same property at every
//! prefix depth `1..=|states|`.
//!
//! The teaching point is the verdict *progression*: below `|S|` the
//! enumeration hasn't reached every simple path, so `incomplete` is an
//! honest unknown, not a failure — and once a violation surfaces it never
//! recedes (deeper sweeps strictly cover more lassos). At depth `|S|` the
//! check is exhaustive: the last row is the real answer.

use serde::Serialize;

use crate::ltl::Formula;
use crate::model::{KripkeSpec, ModelCheckOptions, Verdict, model_check};

/// One row of a [`DepthSweep`]: the verdict at a single prefix depth.
#[derive(Debug, Clone, Serialize)]
pub struct DepthRow {
    pub depth: usize,
    pub verdict: Verdict,
    /// Decided at this depth (not incomplete).
    pub complete: bool,
    pub checked_lassos: usize,
}

/// The result of [`analyze_depth`].
#[derive(Debug, Clone, Serialize)]
pub struct DepthSweep {
    pub states: usize,
    /// One row per depth, `1..=|states|` — the last row is the
    /// exhaustive check.
    pub rows: Vec<DepthRow>,
    /// First depth where a violation surfaces, if any. Violation is
    /// monotone: once found it never recedes.
    pub first_violation_at: Option<usize>,
    /// The exhaustive row (depth = |states|), or `None` for an empty
    /// model.
    pub conclusive: Option<DepthRow>,
}

/// Run [`model_check`] at every prefix depth `1..=|states|` with the same
/// budget and counterexample cap, and report the verdict progression.
pub fn analyze_depth(
    formula: &Formula,
    model: &KripkeSpec,
    opts: &ModelCheckOptions,
) -> DepthSweep {
    let n = model.states.len();
    let rows: Vec<DepthRow> = (1..=n)
        .map(|d| {
            let r = model_check(
                formula,
                model,
                &ModelCheckOptions {
                    depth: Some(d),
                    ..opts.clone()
                },
            );
            DepthRow {
                depth: d,
                verdict: r.verdict,
                complete: r.complete,
                checked_lassos: r.checked_lassos,
            }
        })
        .collect();
    DepthSweep {
        states: n,
        first_violation_at: rows
            .iter()
            .find(|r| r.verdict == Verdict::Violated)
            .map(|r| r.depth),
        conclusive: rows.last().cloned(),
        rows,
    }
}
