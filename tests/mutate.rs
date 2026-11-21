#![cfg(feature = "std")]

use formalmethods::{
    KripkeSpec, MutantOutcome, Mutation, mutants, mutation_report, parse_kripke, parse_ltl,
};
use serde_json::json;

fn vending() -> KripkeSpec {
    let (spec, _warnings) = parse_kripke(&json!({
        "atoms": ["coin", "paid", "vend"],
        "initial": ["idle"],
        "states": [
            { "id": "idle", "atoms": [], "next": ["idle", "paid"] },
            { "id": "paid", "atoms": ["coin", "paid"], "next": ["vend"] },
            { "id": "vend", "atoms": ["vend"], "next": ["idle"] }
        ]
    }))
    .unwrap();
    spec
}

#[test]
fn enumerates_single_point_mutants() {
    let ms = mutants(&vending());
    // idle: 1 relabel×3 atoms + 2 detransitions = 5
    // paid: 2 unlabels + 1 relabel(vend) + 1 detransition = 4
    // vend: 1 unlabel + 2 relabels + 1 detransition = 4
    assert_eq!(ms.len(), 13);
    assert!(ms.contains(&Mutation::Unlabel {
        state: "paid".into(),
        atom: "coin".into()
    }));
    assert!(ms.contains(&Mutation::Detransition {
        state: "paid".into(),
        target: "vend".into()
    }));
}

#[test]
fn strong_suite_kills_label_mutants() {
    // `F vend` is satisfied even if labels are wrong? No — vend is what
    // makes the vend state special. Removing its label kills the liveness.
    let formulas = [parse_ltl("G (coin -> F vend)").unwrap(), parse_ltl("F vend").unwrap()];
    let report = mutation_report(&vending(), &formulas);
    assert_eq!(report.total, 13);
    assert_eq!(report.invalid, 0);
    assert!(
        report
            .mutants
            .iter()
            .any(|m| matches!(
                (&m.mutation, &m.outcome),
                (
                    Mutation::Unlabel {
                        state,
                        atom
                    },
                    MutantOutcome::Killed { .. }
                ) if state == "vend" && atom == "vend"
            )),
        "unlabelling `vend` should be killed by `F vend`"
    );
}

#[test]
fn weak_suite_lets_mutants_survive() {
    // `G (paid -> paid)` holds regardless of transitions out of `paid`;
    // it never notices a removed edge.
    let formulas = [parse_ltl("G true").unwrap()];
    let report = mutation_report(&vending(), &formulas);
    assert_eq!(report.killed, 0);
    assert_eq!(report.survived, 13);
    assert_eq!(report.kill_rate, Some(0.0));
}

#[test]
fn kill_rate_counts_valid_mutants() {
    let formulas = [parse_ltl("G (coin -> X vend)").unwrap()];
    let report = mutation_report(&vending(), &formulas);
    assert!(report.invalid == 0);
    assert_eq!(
        report.kill_rate,
        Some(report.killed as f64 / report.total as f64)
    );
    assert!(report.survived + report.killed == report.total);
}

#[test]
fn mutation_display_is_compact() {
    assert_eq!(
        Mutation::Detransition {
            state: "a".into(),
            target: "b".into()
        }
        .to_string(),
        "detransition a -/-> b"
    );
}
