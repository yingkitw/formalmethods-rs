#![cfg(feature = "std")]

use formalmethods::{
    ModelCheckOptions, Verdict, counterexample_run, falsifying_window, model_check,
    parse_kripke, parse_ltl, satisfies_trace,
};
use serde_json::json;

fn eating() -> formalmethods::KripkeSpec {
    parse_kripke(&json!({
        "atoms": ["ready", "coin", "dispense"],
        "initial": ["idle"],
        "states": [
            { "id": "idle", "atoms": ["ready"], "next": ["paid"] },
            { "id": "paid", "atoms": ["coin"], "next": ["idle", "vend"] },
            { "id": "vend", "atoms": ["dispense"], "next": ["idle"] },
        ],
    }))
    .unwrap()
    .0
}

#[test]
fn unrolls_a_lasso_into_a_trace_that_still_fails_under_ltlf() {
    let f = parse_ltl("G (coin -> F dispense)").unwrap();
    let model = eating();
    let r = model_check(&f, &model, &ModelCheckOptions::default());
    assert_eq!(r.verdict, Verdict::Violated);
    let ce = &r.counterexamples[0];
    let replay = falsifying_window(&f, &counterexample_run(&model, ce), &[])
        .expect("a safety violation replays finitely");
    assert!(!satisfies_trace(&f, &replay.trace));
    // prefix + reps whole loops
    assert_eq!(
        replay.trace.len(),
        ce.cycle_start + replay.reps * (ce.path.len() - ce.cycle_start)
    );
}

#[test]
fn the_window_is_a_pasteable_regression_test() {
    // the success criterion, round-tripped: the replayed trace is an LTLf
    // counterexample you can drop straight into check_ltl
    let f = parse_ltl("G (coin -> F dispense)").unwrap();
    let model = eating();
    let ce = &model_check(&f, &model, &ModelCheckOptions::default()).counterexamples[0];
    let replay = falsifying_window(&f, &counterexample_run(&model, ce), &[]).unwrap();
    assert!(!satisfies_trace(&f, &replay.trace));
}

#[test]
fn says_no_when_the_violation_needs_the_infinite_loop_itself() {
    // b → a → (b): the run flickers, never stabilising on p — but every
    // finite window ends on p, so under LTLf `F G p` looks satisfied.
    let flicker = parse_kripke(&json!({
        "atoms": ["p"],
        "initial": ["b"],
        "states": [
            { "id": "b", "atoms": [], "next": ["a"] },
            { "id": "a", "atoms": ["p"], "next": ["b"] },
        ],
    }))
    .unwrap()
    .0;
    let f = parse_ltl("F G p").unwrap();
    let r = model_check(&f, &flicker, &ModelCheckOptions::default());
    assert_eq!(r.verdict, Verdict::Violated);
    let ce = &r.counterexamples[0];
    assert!(falsifying_window(&f, &counterexample_run(&flicker, ce), &[]).is_none());
}
