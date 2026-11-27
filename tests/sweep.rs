#![cfg(feature = "std")]

use formalmethods::{
    KripkeSpec, ModelCheckOptions, Verdict, analyze_depth, parse_kripke, parse_ltl,
};
use serde_json::json;

fn vending() -> KripkeSpec {
    parse_kripke(&json!({
        "atoms": ["ready", "coin", "dispense"],
        "initial": ["idle"],
        "states": [
            { "id": "idle", "atoms": ["ready"], "next": ["paid"] },
            { "id": "paid", "atoms": ["coin"], "next": ["vend"] },
            { "id": "vend", "atoms": ["dispense"], "next": ["idle"] },
        ],
    }))
    .unwrap()
    .0
}

fn eating() -> KripkeSpec {
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

fn verdicts(s: &formalmethods::DepthSweep) -> Vec<Verdict> {
    s.rows.iter().map(|r| r.verdict).collect()
}

#[test]
fn holds_only_at_the_completeness_threshold() {
    let f = parse_ltl("G (coin -> F dispense)").unwrap();
    let s = analyze_depth(&f, &vending(), &ModelCheckOptions::default());
    assert_eq!(s.states, 3);
    assert_eq!(s.rows.len(), 3);
    assert_eq!(
        verdicts(&s),
        [Verdict::Incomplete, Verdict::Incomplete, Verdict::Holds]
    );
    assert_eq!(s.first_violation_at, None);
    assert_eq!(s.conclusive.as_ref().unwrap().verdict, Verdict::Holds);
    // only the last row is complete — shallower bounds answer honestly
    assert_eq!(s.rows.iter().filter(|r| r.complete).count(), 1);
}

#[test]
fn violations_surface_early_and_never_recede() {
    let f = parse_ltl("G (coin -> F dispense)").unwrap();
    let s = analyze_depth(&f, &eating(), &ModelCheckOptions::default());
    // the eaten-coin lasso has a 2-state prefix — it surfaces at depth 2
    assert_eq!(s.first_violation_at, Some(2));
    assert_eq!(
        verdicts(&s),
        [Verdict::Incomplete, Verdict::Violated, Verdict::Violated]
    );
    assert_eq!(s.conclusive.as_ref().unwrap().verdict, Verdict::Violated);
}

#[test]
fn a_deep_violation_needs_the_full_depth() {
    let model = parse_kripke(&json!({
        "atoms": ["p"],
        "initial": ["s0"],
        "states": [
            { "id": "s0", "atoms": ["p"], "next": ["s1"] },
            { "id": "s1", "atoms": ["p"], "next": ["s2"] },
            { "id": "s2", "atoms": ["p"], "next": ["s3", "s0"] },
            { "id": "s3", "atoms": [], "next": ["s3"] },
        ],
    }))
    .unwrap()
    .0;
    let f = parse_ltl("G p").unwrap();
    let s = analyze_depth(&f, &model, &ModelCheckOptions::default());
    assert_eq!(s.first_violation_at, Some(4));
    assert_eq!(
        s.rows.iter().filter(|r| r.verdict == Verdict::Incomplete).count(),
        3
    );
}

#[test]
fn violation_is_monotone() {
    for src in ["G (coin -> F dispense)", "F dispense", "G (coin -> X dispense)"] {
        let f = parse_ltl(src).unwrap();
        let s = analyze_depth(&f, &eating(), &ModelCheckOptions::default());
        if let Some(idx) = s.rows.iter().position(|r| r.verdict == Verdict::Violated) {
            for r in &s.rows[idx..] {
                assert_eq!(r.verdict, Verdict::Violated, "{src} at depth {}", r.depth);
            }
        }
    }
}

#[test]
fn empty_model_gives_no_rows_and_no_conclusive_answer() {
    let model = KripkeSpec {
        atoms: Default::default(),
        initial: vec![],
        states: vec![],
    };
    let f = parse_ltl("G p").unwrap();
    let s = analyze_depth(&f, &model, &ModelCheckOptions::default());
    assert!(s.rows.is_empty());
    assert!(s.conclusive.is_none());
}
