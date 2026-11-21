use formalmethods::{
    ModelCheckOptions, Verdict, analyze_coverage, model_check, parse_kripke, parse_ltl,
};
use serde_json::json;

fn vending() -> serde_json::Value {
    json!({
        "atoms": ["ready", "coin", "dispense"],
        "initial": ["idle"],
        "states": [
            { "id": "idle", "atoms": ["ready"], "next": ["paid"] },
            { "id": "paid", "atoms": ["coin"], "next": ["vend"] },
            { "id": "vend", "atoms": ["dispense"], "next": ["idle"] },
        ],
    })
}

fn eating() -> serde_json::Value {
    let mut v = vending();
    v["states"][1]["next"] = json!(["idle", "vend"]);
    v
}

mod parse_kripke_tests {
    use super::*;

    #[test]
    fn validates_references() {
        let mut bad_init = vending();
        bad_init["initial"] = json!(["nope"]);
        assert!(
            parse_kripke(&bad_init)
                .unwrap_err()
                .to_string()
                .contains("initial state")
        );

        let mut bad_next = vending();
        bad_next["states"][0]["next"] = json!(["ghost"]);
        assert!(
            parse_kripke(&bad_next)
                .unwrap_err()
                .to_string()
                .contains("ghost")
        );
    }

    #[test]
    fn auto_declares_stray_atoms_with_a_warning() {
        let (model, warnings) = parse_kripke(&json!({
            "atoms": ["a"],
            "initial": ["s0"],
            "states": [{ "id": "s0", "atoms": ["a", "b"], "next": [] }],
        }))
        .unwrap();
        assert!(model.atoms.contains("b"));
        assert!(warnings.iter().any(|w| w.contains("'b'")));
    }
}

mod model_check_tests {
    use super::*;

    #[test]
    fn proves_a_correct_machine_exhaustively() {
        let (model, _) = parse_kripke(&vending()).unwrap();
        let r = model_check(
            &parse_ltl("G (coin -> F dispense)").unwrap(),
            &model,
            &ModelCheckOptions::default(),
        );
        assert_eq!(r.verdict, Verdict::Holds);
        assert!(r.complete);
        assert!(r.counterexamples.is_empty());
    }

    #[test]
    fn finds_a_bug_and_witnesses_it_with_a_lasso() {
        let (model, _) = parse_kripke(&eating()).unwrap();
        let r = model_check(
            &parse_ltl("G (coin -> F dispense)").unwrap(),
            &model,
            &ModelCheckOptions::default(),
        );
        assert_eq!(r.verdict, Verdict::Violated);
        let ce = &r.counterexamples[0];
        assert_eq!(ce.path, ["idle", "paid"]);
        assert_eq!(ce.cycle_start, 0);
        assert!(!ce.window.table.result);
    }

    #[test]
    fn treats_dead_ends_as_stuttering_so_liveness_catches_hangs() {
        let dead = json!({
            "atoms": ["go"],
            "initial": ["s"],
            "states": [{ "id": "s", "atoms": ["go"], "next": [] }],
        });
        let (model, _) = parse_kripke(&dead).unwrap();
        assert_eq!(
            model_check(&parse_ltl("G go").unwrap(), &model, &ModelCheckOptions::default()).verdict,
            Verdict::Holds
        );
        assert_eq!(
            model_check(&parse_ltl("F (!go)").unwrap(), &model, &ModelCheckOptions::default())
                .verdict,
            Verdict::Violated
        );
    }

    #[test]
    fn reports_depth_bounds_honestly_as_incomplete() {
        let chain = json!({
            "atoms": ["stop"],
            "initial": ["s0"],
            "states": [
                { "id": "s0", "atoms": [], "next": ["s1"] },
                { "id": "s1", "atoms": [], "next": ["s2"] },
                { "id": "s2", "atoms": [], "next": ["s3"] },
                { "id": "s3", "atoms": ["stop"], "next": [] },
            ],
        });
        let (model, _) = parse_kripke(&chain).unwrap();
        assert_eq!(
            model_check(
                &parse_ltl("F stop").unwrap(),
                &model,
                &ModelCheckOptions {
                    depth: Some(2),
                    ..Default::default()
                },
            )
            .verdict,
            Verdict::Incomplete
        );
        assert_eq!(
            model_check(&parse_ltl("F stop").unwrap(), &model, &ModelCheckOptions::default())
                .verdict,
            Verdict::Holds
        );
    }
}

mod coverage_tests {
    use super::*;

    fn ids(trace: &[&str]) -> Vec<String> {
        trace.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn counts_states_and_transitions_and_only_that() {
        let (model, _) = parse_kripke(&vending()).unwrap();
        let r = analyze_coverage(&model, &[ids(&["idle", "paid", "vend"])]);
        assert_eq!(r.state_percent, 100.0);
        assert!((r.transition_percent - (2.0 / 3.0) * 100.0).abs() < 1e-9);
        assert_eq!(r.missing_transitions, ["vend -> idle"]);
        assert_eq!(
            analyze_coverage(
                &model,
                &[ids(&["idle", "paid", "vend"]), ids(&["vend", "idle"])]
            )
            .transition_percent,
            100.0
        );
    }

    #[test]
    fn flags_unknown_states_in_traces() {
        let (model, _) = parse_kripke(&vending()).unwrap();
        assert_eq!(
            analyze_coverage(&model, &[ids(&["ghost"])]).unknown_states,
            ["ghost"]
        );
    }
}
