//! Property-based falsification over finite traces.
//!
//! Generates random finite executions (LTLf semantics), checks the property,
//! and — when a violation is found — shrinks it with a simple delta-debugging
//! loop (drop positions, then drop individual atoms).  Deterministic given a
//! seed, so every counterexample is reproducible.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::ltl::{Formula, Trace, eval_at};

/// Small, fast, deterministic PRNG (mulberry32).
///
/// Yields `f64` values in `[0, 1)`, like `Math.random()` — the same algorithm
/// the TypeScript lab uses, so identical seeds produce identical campaigns.
#[derive(Debug, Clone)]
pub struct Mulberry32 {
    a: u32,
}

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Mulberry32 { a: seed }
    }

    /// Next value in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6d2b79f5);
        let mut t = self.a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61)) ^ t;
        (t ^ (t >> 14)) as f64 / 4294967296.0
    }
}

/// Tuning knobs for [`falsify`].
#[derive(Debug, Clone, Default)]
pub struct FalsifyOptions {
    /// Number of random traces to try. Default 300, max 100000.
    pub iterations: Option<usize>,
    /// Length of each random trace. Default 6, max 64.
    pub length: Option<usize>,
    /// RNG seed; omit for a nondeterministic one.
    pub seed: Option<u32>,
}

/// The outcome of a falsification campaign.
#[derive(Debug, Clone, Serialize)]
pub struct FalsifyResult {
    pub violated: bool,
    pub iterations: usize,
    pub length: usize,
    pub seed: u32,
    pub tried: usize,
    /// Shrunk counterexample (minimal violating trace), if any.
    pub counterexample: Option<Trace>,
    /// The first raw violating trace, before shrinking.
    pub original: Option<Trace>,
    pub shrink_steps: Option<usize>,
}

/// Delta-debug a violating trace down toward something small and readable.
///
/// Returns the shrunk trace (which still violates `formula`) and the number
/// of successful shrink steps.
pub fn shrink_trace(trace: &Trace, formula: &Formula) -> (Trace, usize) {
    let mut current: Trace = trace.clone();
    let mut steps = 0usize;
    let violates = |t: &Trace| !eval_at(formula, t, 0);

    let mut progress = true;
    while progress {
        progress = false;
        // Drop whole positions.
        for i in 0..current.len() {
            if current.len() <= 1 {
                break;
            }
            let mut candidate = current.clone();
            candidate.remove(i);
            if violates(&candidate) {
                current = candidate;
                steps += 1;
                progress = true;
                break;
            }
        }
        if progress {
            continue;
        }
        // Drop individual atoms.
        'outer: for i in 0..current.len() {
            let atoms: Vec<String> = current[i].iter().cloned().collect();
            for j in (0..atoms.len()).rev() {
                let mut candidate = current.clone();
                candidate[i].remove(&atoms[j]);
                if violates(&candidate) {
                    current = candidate;
                    steps += 1;
                    progress = true;
                    break 'outer;
                }
            }
        }
    }
    (current, steps)
}

/// Property-based testing for LTL: generate random finite traces over the
/// given atoms, check the formula under LTLf semantics, and shrink the first
/// violation found.
///
/// A `violated: false` result means *not falsified within the campaign* — it
/// is evidence, never proof.
pub fn falsify(formula: &Formula, atoms: &[String], opts: &FalsifyOptions) -> FalsifyResult {
    let iterations = opts.iterations.unwrap_or(300).clamp(1, 100_000);
    let length = opts.length.unwrap_or(6).clamp(1, 64);
    let seed = opts.seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
            .unwrap_or(0x9e3779b9)
    });
    let mut rng = Mulberry32::new(seed);

    let mut tried = 0usize;
    let mut original: Option<Trace> = None;
    while tried < iterations {
        tried += 1;
        let mut trace: Trace = Vec::with_capacity(length);
        for _ in 0..length {
            let mut state = crate::ltl::State::new();
            for a in atoms {
                if rng.next_f64() < 0.5 {
                    state.insert(a.clone());
                }
            }
            trace.push(state);
        }
        if !eval_at(formula, &trace, 0) {
            original = Some(trace);
            break;
        }
    }

    let Some(original) = original else {
        return FalsifyResult {
            violated: false,
            iterations,
            length,
            seed,
            tried,
            counterexample: None,
            original: None,
            shrink_steps: None,
        };
    };
    let (counterexample, steps) = shrink_trace(&original, formula);
    FalsifyResult {
        violated: true,
        iterations,
        length,
        seed,
        tried,
        counterexample: Some(counterexample),
        original: Some(original),
        shrink_steps: Some(steps),
    }
}
