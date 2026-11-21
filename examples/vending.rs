//! End-to-end tour: one spec, three instruments.
//!
//! A vending machine must always eventually dispense after a coin:
//! `G (coin -> F dispense)`. We check a recorded trace, model-check the
//! intended design (and a buggy variant), fuzz the spec, and measure the
//! coverage of the test traces.
//!
//! Run: `cargo run --example vending`

use formalmethods::{
    FalsifyOptions, ModelCheckOptions, Verdict, analyze_coverage, falsify, model_check,
    parse_kripke, parse_ltl, satisfies_trace, trace,
};
use serde_json::json;

fn main() {
    let spec = parse_ltl("G (coin -> F dispense)").unwrap();

    // 1. A recorded run: does this log satisfy the contract? (LTLf)
    let log = trace(&[&["ready"], &["coin"], &["dispense"], &["ready"]]);
    println!("check_ltl:     log satisfies spec = {}", satisfies_trace(&spec, &log));

    // 2. The intended design, checked exhaustively.
    let vending = json!({
        "atoms": ["ready", "coin", "dispense"],
        "initial": ["idle"],
        "states": [
            { "id": "idle", "atoms": ["ready"], "next": ["paid"] },
            { "id": "paid", "atoms": ["coin"], "next": ["vend"] },
            { "id": "vend", "atoms": ["dispense"], "next": ["idle"] },
        ],
    });
    let (model, _) = parse_kripke(&vending).unwrap();
    let r = model_check(&spec, &model, &ModelCheckOptions::default());
    println!("model_check:   design verdict = {} ({} lassos)", r.verdict, r.checked_lassos);

    // 3. A buggy design: `paid` can loop back to `idle` and eat the coin.
    let mut eating = vending.clone();
    eating["states"][1]["next"] = json!(["idle", "vend"]);
    let (buggy, _) = parse_kripke(&eating).unwrap();
    let r = model_check(&spec, &buggy, &ModelCheckOptions::default());
    assert_eq!(r.verdict, Verdict::Violated);
    let ce = &r.counterexamples[0];
    println!(
        "model_check:   buggy verdict = violated, counterexample {:?} (loop at {})",
        ce.path, ce.cycle_start
    );

    // 4. Fuzz the spec itself — is it even falsifiable?
    let r = falsify(
        &spec,
        &["coin".into(), "dispense".into()],
        &FalsifyOptions {
            seed: Some(7),
            iterations: Some(200),
            length: Some(6),
        },
    );
    println!(
        "falsify:       violated = {}, seed = {}, shrunk to {} step(s)",
        r.violated,
        r.seed,
        r.counterexample.as_ref().map_or(0, |t| t.len())
    );

    // 5. Coverage: which transitions do our test traces never exercise?
    let report = analyze_coverage(&model, &[vec!["idle".into(), "paid".into(), "vend".into()]]);
    println!(
        "coverage:      states {:.0}%, transitions {:.0}% — missing {:?}",
        report.state_percent, report.transition_percent, report.missing_transitions
    );
}
