#![cfg(feature = "std")]

use formalmethods::{
    Formula, ModelCheckOptions, PeriodicTrace, SatOptions, Verdict, eval_periodic_at,
    model_check, parse_ltl, presets, sat_check,
};

fn bench() -> Vec<formalmethods::KripkeSpec> {
    vec![presets::vending(), presets::vending_eaten(), presets::stuck_light()]
}

/// Every grammar construct, exercised across all three presets: the SAT
/// verdict must agree with the explicit checker's wherever the explicit
/// checker is conclusive. Counterexample *paths* may differ (DFS order vs
/// shortest-first) — verdicts may not.
#[test]
fn agrees_with_model_check_across_the_grammar() {
    let formulas = [
        "ready", "!ready", "coin && ready", "coin || dispense",
        "coin -> dispense", "coin <-> !dispense",
        "X coin", "G (coin -> F dispense)", "F dispense",
        "ready U coin", "ready W coin", "ready R coin",
        "Y ready", "O coin", "H (coin -> coin)", "ready S coin",
        "G (ready -> X coin)", "F G stop", "G F go",
        "G ((coin && !dispense) -> X (dispense || coin))",
    ];
    for spec in bench() {
        for src in formulas {
            let f = parse_ltl(src).unwrap();
            let mc = model_check(&f, &spec, &ModelCheckOptions::default());
            let sat = sat_check(&f, &spec, &SatOptions::default());
            if mc.verdict == Verdict::Incomplete {
                continue; // truncation — nothing to agree with
            }
            assert_eq!(
                sat.verdict, mc.verdict,
                "{src} on {}-state model",
                spec.states.len()
            );
        }
    }
}

/// A reported counterexample must be a real violating lasso — replay it
/// through the evaluator ourselves.
#[test]
fn returned_counterexamples_actually_violate() {
    let spec = presets::vending_eaten();
    let f = parse_ltl("G (coin -> F dispense)").unwrap();
    let r = sat_check(&f, &spec, &SatOptions::default());
    assert_eq!(r.verdict, Verdict::Violated);
    let ce = r.counterexample.unwrap();
    let atoms_of = |id: &str| {
        spec.states
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.atoms.clone())
            .unwrap_or_default()
    };
    let pt = PeriodicTrace {
        prefix: ce.path[..ce.cycle_start].iter().map(|s| atoms_of(s)).collect(),
        cycle: ce.path[ce.cycle_start..].iter().map(|s| atoms_of(s)).collect(),
    };
    assert!(!eval_periodic_at(&f, &pt, 0));
}

/// depth below |states| can never report `holds` — even when nothing
/// violates, the honest answer is `incomplete`.
#[test]
fn shallow_depth_is_never_conclusive() {
    let spec = presets::vending();
    let f = parse_ltl("G (ready || coin || dispense)").unwrap();
    let full = sat_check(&f, &spec, &SatOptions::default());
    assert_eq!(full.verdict, Verdict::Holds);
    let shallow = sat_check(&f, &spec, &SatOptions { depth: Some(2) });
    assert_eq!(shallow.verdict, Verdict::Incomplete);
    assert!(shallow.notes.iter().any(|n| n.contains("< |states|")));
}

/// A model with no initial states has no lassos — vacuous `holds` at full
/// depth, matching the explicit checker.
#[test]
fn empty_initial_is_vacuous() {
    let mut spec = presets::vending();
    spec.initial.clear();
    let f = parse_ltl("F dispense").unwrap();
    assert_eq!(sat_check(&f, &spec, &SatOptions::default()).verdict, Verdict::Holds);
    assert_eq!(
        model_check(&f, &spec, &ModelCheckOptions::default()).verdict,
        Verdict::Holds
    );
}

/// Dead ends stutter: a state with no successors self-loops, so `X p` sees
/// the frozen state — the same convention as `model_check`/`ctl_check`.
#[test]
fn dead_ends_stutter() {
    use formalmethods::{KripkeSpec, KripkeState};
    let spec = KripkeSpec {
        atoms: ["p"].iter().map(|s| s.to_string()).collect(),
        initial: vec!["a".into()],
        states: vec![
            KripkeState { id: "a".into(), atoms: Default::default(), next: vec!["b".into()] },
            KripkeState {
                id: "b".into(),
                atoms: ["p"].iter().map(|s| s.to_string()).collect(),
                next: vec![],
            },
        ],
    };
    // At the dead end the run freezes on `p` — `G (p -> X p)` holds.
    let f = parse_ltl("G (p -> X p)").unwrap();
    assert_eq!(sat_check(&f, &spec, &SatOptions::default()).verdict, Verdict::Holds);
    // A `p`-then-not-`p` step is impossible under stuttering — the promise
    // `F (p && X !p)` never fires, so it is *violated* on every lasso.
    let f2 = parse_ltl("F (p && X !p)").unwrap();
    assert_eq!(
        sat_check(&f2, &spec, &SatOptions::default()).verdict,
        Verdict::Violated
    );
}

/// The shortest-first enumeration: the found lasso should be no longer
/// than the explicit checker's first counterexample.
#[test]
fn finds_short_counterexamples() {
    let spec = presets::stuck_light();
    let f = parse_ltl("F G go").unwrap(); // violated: can stay red forever
    let r = sat_check(&f, &spec, &SatOptions::default());
    assert_eq!(r.verdict, Verdict::Violated);
    let ce = r.counterexample.unwrap();
    assert_eq!(ce.path.len(), 1, "the red self-loop is a 1-position lasso");
    assert_eq!(ce.path, ["red"]);
}

/// `Formula` must be `PartialEq`-comparable across the two engines' results
/// — and both engines must report the same verdict on a tricky nesting.
#[test]
fn tricky_nesting_agrees() {
    let spec = presets::vending_eaten();
    for src in ["F (coin U dispense)", "G (coin R (ready || dispense))", "F (coin S ready)"] {
        let f = parse_ltl(src).unwrap();
        let mc = model_check(&f, &spec, &ModelCheckOptions::default());
        let sat = sat_check(&f, &spec, &SatOptions::default());
        assert_eq!(sat.verdict, mc.verdict, "{src}");
    }
    let _ = Formula::True; // silence unused-import lints if assertion set changes
}
