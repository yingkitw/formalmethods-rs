//! Property-based falsification over finite traces.
//!
//! Generates random finite executions (LTLf semantics), checks the property,
//! and — when a violation is found — shrinks it with a simple delta-debugging
//! loop (drop positions, then drop individual atoms).  Deterministic given a
//! seed, so every counterexample is reproducible.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, collections::BTreeSet, string::String, vec::Vec};
#[cfg(feature = "std")]
use std::{collections::BTreeSet, time::{SystemTime, UNIX_EPOCH}};

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
    /// Coverage-guided: keep a corpus of traces that each introduced an
    /// unseen state signature, and generate half the campaign by mutating
    /// corpus entries. Biases exploration toward state combinations not
    /// yet tried — novelty seeking over the 2^|atoms| space.
    pub guided: Option<bool>,
}

/// Seed used when `FalsifyOptions::seed` is `None`. With `std`, derived
/// from the wall clock (nondeterministic); without `std` there is no clock,
/// so it falls back to a fixed constant — pass an explicit `seed` for
/// variety in `no_std` builds.
#[cfg(feature = "std")]
fn default_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
        .unwrap_or(0x9e3779b9)
}

#[cfg(not(feature = "std"))]
fn default_seed() -> u32 {
    0x9e3779b9
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
    /// Present when `guided` — how much of the atom-set space the
    /// campaign touched.
    pub coverage: Option<GuidedCoverage>,
}

/// Coverage stats from a guided campaign: the fraction of the 2^|atoms|
/// state space the corpus actually saw.
#[derive(Debug, Clone, Serialize)]
pub struct GuidedCoverage {
    /// Distinct state signatures seen.
    pub states_seen: usize,
    /// 2^|atoms|; `None` when the alphabet is too big for a meaningful
    /// ratio (>20).
    pub states_possible: Option<u64>,
    /// Traces held in the novelty corpus (capped at 128).
    pub corpus_size: usize,
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

/// Formula-aware shrinking: [`shrink_trace`] plus a provably-safe first
/// pass that drops every atom the formula never mentions.
///
/// Atoms absent from `formula` are unobservable — removing them cannot
/// change `eval_at`, so the purge preserves any violation *for free*.
/// What remains is then delta-debugged as usual, so the result is at
/// least as small and strictly more readable: counterexamples over wide
/// alphabets lose their noise atoms immediately.
///
/// Compose rather than reimplement: `falsify` deliberately stays
/// semantics-blind (TS parity); callers wanting the tighter shrink run
/// this on the returned counterexample.
pub fn shrink_trace_for(trace: &Trace, formula: &Formula) -> (Trace, usize) {
    let readable = crate::ltl::collect_atoms(formula);
    let stripped: Trace = trace
        .iter()
        .map(|s| {
            s.iter()
                .filter(|a| readable.contains(*a))
                .cloned()
                .collect()
        })
        .collect();
    shrink_trace(&stripped, formula)
}

/// Generate a random formula over `atoms`, depth-bounded — randltl-style.
///
/// Covers the whole grammar, past-time operators included; `max_depth`
/// bounds AST depth (a leaf counts as depth 0). Deterministic given the
/// `rng` — the companion to `parse_ltl`/`print_ltl` round-trip fuzzing
/// and to conjuring random properties to probe a spec with.
pub fn gen_formula(atoms: &[String], max_depth: usize, rng: &mut Mulberry32) -> Formula {
    fn leaf(atoms: &[String], rng: &mut Mulberry32) -> Formula {
        if !atoms.is_empty() && rng.next_f64() < 0.8 {
            Formula::Atom(atoms[(rng.next_f64() * atoms.len() as f64) as usize].clone())
        } else if rng.next_f64() < 0.5 {
            Formula::True
        } else {
            Formula::False
        }
    }
    if max_depth == 0 {
        return leaf(atoms, rng);
    }
    let d = max_depth - 1;
    let sub = |rng: &mut Mulberry32| Box::new(gen_formula(atoms, d, rng));
    let bin = |rng: &mut Mulberry32| (sub(rng), sub(rng));
    match (rng.next_f64() * 18.0) as usize {
        0..=2 => leaf(atoms, rng),
        3 => Formula::Not(sub(rng)),
        4 => Formula::Next(sub(rng)),
        5 => Formula::Globally(sub(rng)),
        6 => Formula::Finally(sub(rng)),
        7 => Formula::Yesterday(sub(rng)),
        8 => Formula::Once(sub(rng)),
        9 => Formula::Historically(sub(rng)),
        10 => {
            let (l, r) = bin(rng);
            Formula::And(l, r)
        }
        11 => {
            let (l, r) = bin(rng);
            Formula::Or(l, r)
        }
        12 => {
            let (l, r) = bin(rng);
            Formula::Implies(l, r)
        }
        13 => {
            let (l, r) = bin(rng);
            Formula::Iff(l, r)
        }
        14 => {
            let (l, r) = bin(rng);
            Formula::Until(l, r)
        }
        15 => {
            let (l, r) = bin(rng);
            Formula::Weak(l, r)
        }
        16 => {
            let (l, r) = bin(rng);
            Formula::Release(l, r)
        }
        _ => {
            let (l, r) = bin(rng);
            Formula::Since(l, r)
        }
    }
}

/// One bounded mutation of a corpus trace: drop a position, duplicate
/// one, or toggle an atom.
fn mutate_trace(trace: &Trace, atoms: &[String], rng: &mut Mulberry32, length: usize) -> Trace {
    let mut t = trace.clone();
    let op = (rng.next_f64() * 3.0) as usize;
    if op == 0 && t.len() > 1 {
        t.remove((rng.next_f64() * t.len() as f64) as usize);
    } else if op == 1 && t.len() < length {
        let i = (rng.next_f64() * t.len() as f64) as usize;
        t.insert(i, t[i].clone());
    } else {
        let i = (rng.next_f64() * t.len() as f64) as usize;
        let a = &atoms[(rng.next_f64() * atoms.len() as f64) as usize];
        if !t[i].remove(a) {
            t[i].insert(a.clone());
        }
    }
    t
}

/// A state's novelty signature: its atoms, sorted, comma-joined — the same
/// canonical form the TS engine uses for the coverage corpus.
fn signature(state: &crate::ltl::State) -> String {
    state.iter().cloned().collect::<Vec<_>>().join(",")
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
    let seed = opts.seed.unwrap_or_else(default_seed);
    let mut rng = Mulberry32::new(seed);
    let guided = opts.guided.unwrap_or(false);

    // Coverage corpus: traces that each introduced an unseen signature.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut corpus: Vec<Trace> = Vec::new();

    let fresh = |rng: &mut Mulberry32| -> Trace {
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
        trace
    };

    let mut tried = 0usize;
    let mut original: Option<Trace> = None;
    while tried < iterations {
        tried += 1;
        // `fresh` draws unconditionally — even when the trace is then
        // replaced by a corpus mutation — keeping the RNG stream aligned
        // with the TS engine bit for bit.
        let mut trace = fresh(&mut rng);
        if guided {
            if !corpus.is_empty() && rng.next_f64() < 0.5 {
                let pick = (rng.next_f64() * corpus.len() as f64) as usize;
                trace = mutate_trace(&corpus[pick], atoms, &mut rng, length);
            }
            if trace.iter().any(|s| !seen.contains(&signature(s))) {
                for s in &trace {
                    seen.insert(signature(s));
                }
                corpus.push(trace.clone());
                if corpus.len() > 128 {
                    corpus.remove(0);
                }
            }
        }
        if !eval_at(formula, &trace, 0) {
            original = Some(trace);
            break;
        }
    }

    let coverage = guided.then_some(GuidedCoverage {
        states_seen: seen.len(),
        states_possible: (atoms.len() <= 20).then(|| 1u64 << atoms.len()),
        corpus_size: corpus.len(),
    });

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
            coverage,
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
        coverage,
    }
}
