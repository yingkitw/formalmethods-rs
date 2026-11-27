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

// ---------------------------------------------------------------------------
// Formula mutants — mirrors the TS mutation.test.ts suite
// ---------------------------------------------------------------------------

mod formula_mutants {
    use super::*;
    use formalmethods::{
        ModelCheckOptions, MutantStatus, SpecMutantKind, Verdict, analyze_spec_coverage,
        ast_size, operator_mutants, print_ltl, spec_mutants,
    };

    fn ts_vending() -> KripkeSpec {
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

    fn ts_eating() -> KripkeSpec {
        let mut v = ts_vending();
        v.states[1].next = vec!["idle".into(), "vend".into()];
        v
    }

    #[test]
    fn spec_mutants_flip_every_position_smallest_first_root_last() {
        let f = parse_ltl("G (coin -> F dispense)").unwrap();
        let ms = spec_mutants(&f);
        assert_eq!(ms.len(), ast_size(&f));
        let targets: Vec<&str> = ms.iter().map(|m| m.target.as_str()).collect();
        assert_eq!(
            targets,
            [
                "coin",
                "dispense",
                "F dispense",
                "coin -> F dispense",
                "G (coin -> F dispense)"
            ]
        );
    }

    #[test]
    fn spec_mutants_keep_positions_distinct() {
        let f = parse_ltl("a && a").unwrap();
        let mutations: Vec<String> = spec_mutants(&f)
            .iter()
            .map(|m| print_ltl(&parse_ltl(&m.mutation).unwrap()))
            .collect();
        assert_eq!(mutations, ["!a && a", "a && !a", "!(a && a)"]);
    }

    #[test]
    fn spec_mutants_parse_and_print_back_verbatim() {
        let f = parse_ltl("G (coin -> F dispense)").unwrap();
        for m in spec_mutants(&f) {
            assert_eq!(print_ltl(&parse_ltl(&m.mutation).unwrap()), m.mutation);
        }
    }

    #[test]
    fn operator_mutants_never_produce_entailed_weakenings() {
        // G→F, U→W, ∧→∨ would always survive where the base holds — noise.
        for src in ["G p", "a U b", "a && b", "a R b", "H q", "a S b"] {
            assert!(
                operator_mutants(&parse_ltl(src).unwrap()).is_empty(),
                "{src} should admit no operator mutant"
            );
        }
    }

    #[test]
    fn operator_mutants_swap_each_admitted_operator() {
        let ms = operator_mutants(&parse_ltl("G (coin -> F dispense)").unwrap());
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].kind, SpecMutantKind::Strengthen);
        assert_eq!(ms[0].target, "F dispense → G dispense");
        assert_eq!(ms[0].mutation, "G (coin -> G dispense)");

        let drops = operator_mutants(&parse_ltl("a || b").unwrap());
        let summary: Vec<String> = drops
            .iter()
            .map(|m| format!("{}:{}", m.kind.as_str(), m.target))
            .collect();
        assert_eq!(
            summary,
            [
                "drop:a || b → a",
                "strengthen:a || b → a && b",
                "drop:a || b → b"
            ]
        );
    }

    #[test]
    fn operator_mutants_parse_and_print_back_verbatim() {
        let f = parse_ltl("G (a W b) && F (X c || O d)").unwrap();
        for m in operator_mutants(&f) {
            assert_eq!(print_ltl(&parse_ltl(&m.mutation).unwrap()), m.mutation);
        }
    }

    #[test]
    fn scores_a_passing_spec_killed_witnessed_survivors_named() {
        let f = parse_ltl("G (coin -> F dispense)").unwrap();
        let cov = analyze_spec_coverage(&f, &ts_vending(), &ModelCheckOptions::default(), None);
        assert_eq!(cov.base_verdict, Verdict::Holds);
        assert_eq!(cov.total, 6);
        assert_eq!(cov.killed, 4);
        assert_eq!(cov.survived, 2);
        assert_eq!(cov.unknown, 0);
        assert_eq!(cov.score, Some(66.7));

        let survivors: Vec<&str> = cov
            .mutants
            .iter()
            .filter(|m| m.status == MutantStatus::Survived)
            .map(|m| m.target.as_str())
            .collect();
        // the coin guard and the dispense atom are not load-bearing here
        assert_eq!(survivors, ["coin", "dispense"]);

        let killed: Vec<_> = cov
            .mutants
            .iter()
            .filter(|m| m.status == MutantStatus::Killed)
            .collect();
        for k in &killed {
            assert_eq!(k.verdict, Verdict::Violated);
            assert!(k.witness.is_some());
        }
        assert_eq!(killed[0].witness.as_ref().unwrap().path, ["idle", "paid", "vend"]);
        assert_eq!(killed[0].witness.as_ref().unwrap().cycle_start, 0);
    }

    #[test]
    fn tautology_free_safety_property_is_fully_covered() {
        let f = parse_ltl("G (ready -> X (!ready))").unwrap();
        let cov = analyze_spec_coverage(&f, &ts_vending(), &ModelCheckOptions::default(), None);
        assert_eq!(cov.base_verdict, Verdict::Holds);
        assert_eq!(cov.score, Some(100.0));
        assert_eq!(cov.survived, 0);
    }

    #[test]
    fn refuses_to_score_a_failing_base() {
        let f = parse_ltl("G (coin -> F dispense)").unwrap();
        let cov = analyze_spec_coverage(&f, &ts_eating(), &ModelCheckOptions::default(), None);
        assert_eq!(cov.base_verdict, Verdict::Violated);
        assert!(cov.mutants.is_empty());
        assert_eq!(cov.score, None);
        assert!(cov.notes.iter().any(|n| n.contains("base check must hold")));
    }

    #[test]
    fn surviving_strengthen_means_the_operator_was_unexercised() {
        // p holds everywhere: `F p` passes vacuously — every negation is
        // killed, but `G p` holds too: the model cannot distinguish
        // "eventually" from "always".
        let p_everywhere = parse_kripke(&json!({
            "atoms": ["p"],
            "initial": ["s"],
            "states": [{ "id": "s", "atoms": ["p"], "next": ["s"] }],
        }))
        .unwrap()
        .0;
        let f = parse_ltl("F p").unwrap();
        let cov = analyze_spec_coverage(&f, &p_everywhere, &ModelCheckOptions::default(), None);
        let strengthen = cov
            .mutants
            .iter()
            .find(|m| m.kind == SpecMutantKind::Strengthen)
            .unwrap();
        assert_eq!(strengthen.target, "F p → G p");
        assert_eq!(strengthen.status, MutantStatus::Survived);
        assert!(
            cov.mutants
                .iter()
                .filter(|m| m.kind == SpecMutantKind::Negation)
                .all(|m| m.status == MutantStatus::Killed)
        );
    }

    #[test]
    fn operand_drops_name_the_disjunct_that_tested_nothing() {
        // a holds, b never: `a || b` survives dropping to `a` — the `|| b`
        // clause is dead weight — but dropping to `b` and strengthening to
        // `a && b` are killed.
        let only_a = parse_kripke(&json!({
            "atoms": ["a", "b"],
            "initial": ["s"],
            "states": [{ "id": "s", "atoms": ["a"], "next": ["s"] }],
        }))
        .unwrap()
        .0;
        let f = parse_ltl("a || b").unwrap();
        let cov = analyze_spec_coverage(&f, &only_a, &ModelCheckOptions::default(), None);
        let status_of = |target: &str| {
            cov.mutants
                .iter()
                .find(|m| m.target == target)
                .map(|m| m.status)
        };
        assert_eq!(status_of("a || b → a"), Some(MutantStatus::Survived));
        assert_eq!(status_of("a || b → b"), Some(MutantStatus::Killed));
        assert_eq!(status_of("a || b → a && b"), Some(MutantStatus::Killed));
    }

    #[test]
    fn weak_until_strengthened_to_until_is_killed_when_distinguished() {
        // a holds forever, b never arrives — the machine can tell W from U.
        let forever_a = parse_kripke(&json!({
            "atoms": ["a", "b"],
            "initial": ["s"],
            "states": [{ "id": "s", "atoms": ["a"], "next": ["s"] }],
        }))
        .unwrap()
        .0;
        let f = parse_ltl("a W b").unwrap();
        let cov = analyze_spec_coverage(&f, &forever_a, &ModelCheckOptions::default(), None);
        let m = cov.mutants.iter().find(|m| m.target == "a W b → a U b").unwrap();
        assert_eq!(m.status, MutantStatus::Killed);
    }

    #[test]
    fn dropping_next_asks_now_instead_of_next() {
        let p_next = parse_kripke(&json!({
            "atoms": ["p"],
            "initial": ["s0"],
            "states": [
                { "id": "s0", "atoms": [], "next": ["s1"] },
                { "id": "s1", "atoms": ["p"], "next": ["s1"] },
            ],
        }))
        .unwrap()
        .0;
        let f = parse_ltl("X p").unwrap();
        let cov = analyze_spec_coverage(&f, &p_next, &ModelCheckOptions::default(), None);
        let m = cov.mutants.iter().find(|m| m.target == "X p → p").unwrap();
        assert_eq!(m.status, MutantStatus::Killed);
    }

    #[test]
    fn flags_a_vacuous_spec_clause() {
        // `req` never holds on the model, so flipping it can't be detected.
        let quiet = parse_kripke(&json!({
            "atoms": ["req", "resp"],
            "initial": ["s"],
            "states": [{ "id": "s", "atoms": ["resp"], "next": ["s"] }],
        }))
        .unwrap()
        .0;
        let f = parse_ltl("G (req -> F resp)").unwrap();
        let cov = analyze_spec_coverage(&f, &quiet, &ModelCheckOptions::default(), None);
        assert!(cov.survived > 0);
        let req_mutant = cov.mutants.iter().find(|m| m.target == "req").unwrap();
        assert_eq!(req_mutant.status, MutantStatus::Survived);
    }
}
