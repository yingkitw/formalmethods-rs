//! From native code to a verdict — the worked-example pipeline.
//!
//! A vending machine written as a plain Rust state machine, then
//! *mechanically lifted* into a Kripke structure: every enum state becomes
//! a node, every (state, event) outcome an edge, and the state's
//! observables its atoms. The artifact is derived, never transcribed —
//! so it cannot drift from the code it describes.
//!
//! This is the "approach" workflow end to end:
//! real code → artifact (model or trace) → LTL → a labelled verdict.
//!
//! Run: `cargo run --example from_code`

use std::collections::BTreeSet;

use formalmethods::{
    FalsifyOptions, KripkeSpec, KripkeState, ModelCheckOptions, Verdict, falsify,
    model_check, parse_ltl,
};

// --- the system under test, as it is really written ---------------------

#[derive(Clone, Copy)]
enum Vend {
    Idle,
    Paid,
    Vend,
}

/// The environment's input vocabulary — nondeterminism is where the
/// checker gets to explore.
const EVENTS: [&str; 2] = ["pay", "noop"];

impl Vend {
    const ALL: [Vend; 3] = [Vend::Idle, Vend::Paid, Vend::Vend];

    /// One transition step: coin in → always eventually dispenses.
    fn step(self, event: &str) -> Vend {
        match (self, event) {
            (Vend::Idle, "pay") => Vend::Paid,
            (Vend::Paid, _) => Vend::Vend,
            _ => Vend::Idle,
        }
    }

    /// Buggy variant: `paid` on a quiet step returns to `idle` — the coin
    /// is eaten. One line of code changes the whole reachable graph.
    fn step_buggy(self, event: &str) -> Vend {
        match (self, event) {
            (Vend::Idle, "pay") => Vend::Paid,
            (Vend::Paid, "noop") => Vend::Idle,
            (Vend::Paid, _) => Vend::Vend,
            _ => Vend::Idle,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Vend::Idle => "idle",
            Vend::Paid => "paid",
            Vend::Vend => "vend",
        }
    }

    /// The observable atoms of a state — what a log line would record.
    fn atoms(self) -> BTreeSet<String> {
        let a: &[&str] = match self {
            Vend::Idle => &["ready"],
            Vend::Paid => &["coin"],
            Vend::Vend => &["dispense"],
        };
        a.iter().map(|s| s.to_string()).collect()
    }
}

/// Lift a state machine into a Kripke structure: enum states → nodes,
/// (state, event) outcomes → edges.
fn lift(step: fn(Vend, &str) -> Vend) -> KripkeSpec {
    let states = Vend::ALL
        .iter()
        .map(|&s| KripkeState {
            id: s.name().into(),
            atoms: s.atoms(),
            next: EVENTS
                .iter()
                .map(|e| step(s, e).name().to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
        .collect();
    KripkeSpec {
        atoms: ["ready", "coin", "dispense"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        initial: vec!["idle".into()],
        states,
    }
}

fn main() {
    let property = parse_ltl("G (coin -> F dispense)").unwrap();

    // The healthy machine: every inserted coin is eventually answered.
    let ok = lift(Vend::step);
    let r = model_check(&property, &ok, &ModelCheckOptions::default());
    assert_eq!(r.verdict, Verdict::Holds);
    println!("lifted model:   design verdict = {} ({} lassos)", r.verdict, r.checked_lassos);

    // The coin-eating variant: the code changed, so the model did too.
    let buggy = lift(Vend::step_buggy);
    let r = model_check(&property, &buggy, &ModelCheckOptions::default());
    assert_eq!(r.verdict, Verdict::Violated);
    let ce = &r.counterexamples[0];
    println!(
        "lifted model:   buggy verdict = violated — {:?} (loop at {})",
        ce.path, ce.cycle_start
    );

    // The proptest side: the same property over generated logs (LTLf).
    let atoms = ["coin", "dispense", "timeout"].map(String::from);
    let r = falsify(
        &property,
        &atoms,
        &FalsifyOptions { seed: Some(7), iterations: Some(300), ..Default::default() },
    );
    println!(
        "falsify:        violated = {} ({} tries, seed {})",
        r.violated, r.tried, r.seed
    );
}
