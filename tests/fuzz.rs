use formalmethods::{FalsifyOptions, Mulberry32, falsify, parse_ltl, shrink_trace, trace};

fn atoms(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

mod falsify_tests {
    use super::*;

    #[test]
    fn breaks_a_plausible_contract_and_shrinks_the_witness() {
        let f = parse_ltl("G (request -> F response)").unwrap();
        let r = falsify(
            &f,
            &atoms(&["request", "response", "timeout"]),
            &FalsifyOptions {
                seed: Some(424242),
                iterations: Some(200),
                length: Some(6),
                ..Default::default()
            },
        );
        assert!(r.violated);
        assert!(r.counterexample.as_ref().unwrap().len() <= r.original.as_ref().unwrap().len());
        assert!(r.shrink_steps.unwrap_or(0) > 0);
    }

    #[test]
    fn is_reproducible_from_a_seed() {
        let f = parse_ltl("G (request -> F response)").unwrap();
        let opts = FalsifyOptions {
            seed: Some(99),
            iterations: Some(100),
            length: Some(6),
            ..Default::default()
        };
        let a = falsify(&f, &atoms(&["request", "response"]), &opts);
        let b = falsify(&f, &atoms(&["request", "response"]), &opts);
        assert_eq!(a.counterexample, b.counterexample);
        assert_eq!(a.tried, b.tried);
    }

    #[test]
    fn cannot_falsify_a_tautology_and_says_so_honestly() {
        let r = falsify(
            &parse_ltl("F (a || !a)").unwrap(),
            &atoms(&["a"]),
            &FalsifyOptions {
                iterations: Some(50),
                length: Some(4),
                ..Default::default()
            },
        );
        assert!(!r.violated);
        assert_eq!(r.tried, 50);
    }
}

mod guided_tests {
    use super::*;

    #[test]
    fn reports_coverage_stats_over_the_state_space() {
        let r = falsify(
            &parse_ltl("F (a || !a)").unwrap(), // tautology: full campaign runs
            &atoms(&["a", "b"]),
            &FalsifyOptions {
                seed: Some(7),
                iterations: Some(60),
                length: Some(4),
                guided: Some(true),
                atom_density: None,
            },
        );
        let cov = r.coverage.expect("guided campaigns report coverage");
        assert_eq!(cov.states_possible, Some(4)); // 2^2 atoms
        assert!(cov.states_seen >= 1 && cov.states_seen <= 4);
        assert!(cov.corpus_size >= 1 && cov.corpus_size <= 128);
        assert!(cov.corpus_size <= cov.states_seen);
    }

    #[test]
    fn omits_coverage_when_not_guided() {
        let r = falsify(
            &parse_ltl("F (a || !a)").unwrap(),
            &atoms(&["a"]),
            &FalsifyOptions {
                seed: Some(7),
                iterations: Some(10),
                length: Some(2),
                ..Default::default()
            },
        );
        assert!(r.coverage.is_none());
    }

    #[test]
    fn is_deterministic_from_a_seed() {
        let opts = FalsifyOptions {
            seed: Some(31337),
            iterations: Some(80),
            length: Some(5),
            guided: Some(true),
            atom_density: None,
        };
        let f = parse_ltl("G (a -> F b)").unwrap();
        let xs = atoms(&["a", "b", "c"]);
        let a = falsify(&f, &xs, &opts);
        let b = falsify(&f, &xs, &opts);
        assert_eq!(a.violated, b.violated);
        assert_eq!(a.tried, b.tried);
        assert_eq!(
            a.coverage.as_ref().map(|c| c.states_seen),
            b.coverage.as_ref().map(|c| c.states_seen)
        );
    }

    #[test]
    fn still_falsifies_and_shrinks() {
        let r = falsify(
            &parse_ltl("G (a -> b)").unwrap(),
            &atoms(&["a", "b"]),
            &FalsifyOptions {
                seed: Some(424242),
                iterations: Some(200),
                length: Some(4),
                guided: Some(true),
                atom_density: None,
            },
        );
        assert!(r.violated);
        assert!(r.coverage.is_some());
        let ce = r.counterexample.unwrap();
        assert!(ce.iter().any(|s| s.contains("a") && !s.contains("b")));
    }

    #[test]
    fn states_possible_is_capped_for_large_alphabets() {
        let many: Vec<String> = (0..25).map(|i| format!("a{i}")).collect();
        let r = falsify(
            &parse_ltl("F (a0 || !a0)").unwrap(),
            &many,
            &FalsifyOptions {
                seed: Some(1),
                iterations: Some(10),
                length: Some(2),
                guided: Some(true),
                atom_density: None,
            },
        );
        assert_eq!(r.coverage.unwrap().states_possible, None);
    }
}

mod shrink_tests {
    use super::*;

    #[test]
    fn reduces_a_noisy_failing_trace_to_the_minimum() {
        let (shrunk, steps) = shrink_trace(
            &trace(&[&["request"], &["request", "timeout"], &[], &[]]),
            &parse_ltl("G (request -> F response)").unwrap(),
        );
        assert_eq!(shrunk, trace(&[&["request"]]));
        assert!(steps > 0);
    }

    #[test]
    fn never_returns_something_that_satisfies() {
        let formula = parse_ltl("G (a -> F b)").unwrap();
        let (shrunk, _) = shrink_trace(&trace(&[&["a"], &["a"], &[]]), &formula);
        // G (a -> F b) fails iff some a has no later b
        let has_bad = shrunk
            .iter()
            .enumerate()
            .any(|(i, s)| s.contains("a") && !shrunk[i..].iter().any(|t| t.contains("b")));
        assert!(has_bad);
    }

    #[test]
    fn formula_aware_shrink_drops_unread_atoms_for_free() {
        // noise atoms the formula can't observe are stripped before the
        // delta-debug even starts — same violation, cleaner witness
        let f = parse_ltl("G (a -> F b)").unwrap();
        let noisy = trace(&[
            &["a", "noise1", "noise2"],
            &["noise3"],
            &["a", "noise1"],
        ]);
        let (shrunk, _) = formalmethods::shrink_trace_for(&noisy, &f);
        assert!(!formalmethods::satisfies_trace(&f, &shrunk));
        for s in &shrunk {
            for a in s {
                assert!(a == "a" || a == "b", "unread atom {a} survived");
            }
        }
    }
}

mod mulberry32_tests {
    use super::*;

    #[test]
    fn is_deterministic() {
        let mut a = Mulberry32::new(7);
        let mut b = Mulberry32::new(7);
        let va: Vec<f64> = (0..5).map(|_| a.next_f64()).collect();
        let vb: Vec<f64> = (0..5).map(|_| b.next_f64()).collect();
        assert_eq!(va, vb);
    }
}

mod gen_formula_tests {
    use super::*;
    use formalmethods::{eval_at, gen_formula, print_ltl, trace};

    #[test]
    fn is_deterministic_given_a_seed() {
        let a = atoms(&["p", "q"]);
        let f1 = gen_formula(&a, 3, &mut Mulberry32::new(9));
        let f2 = gen_formula(&a, 3, &mut Mulberry32::new(9));
        assert_eq!(print_ltl(&f1), print_ltl(&f2));
    }

    #[test]
    fn printed_formulas_reparse_to_the_same_ast() {
        // The property that guards the printer: every generated formula,
        // across the whole grammar (past-time operators included), must
        // survive a print -> parse round trip unchanged.
        let a = atoms(&["p", "q", "r"]);
        for seed in 0..400u32 {
            let f = gen_formula(&a, 3, &mut Mulberry32::new(seed));
            let text = print_ltl(&f);
            let reparsed = parse_ltl(&text).unwrap_or_else(|e| panic!("seed {seed}: {text}: {e}"));
            assert_eq!(reparsed, f, "seed {seed}: {text}");
        }
    }

    #[test]
    fn generated_formulas_evaluate_without_panicking() {
        let a = atoms(&["p", "q"]);
        let t = trace(&[&["p"], &[], &["p", "q"], &["q"]]);
        for seed in 0..200u32 {
            let f = gen_formula(&a, 4, &mut Mulberry32::new(seed));
            let _ = eval_at(&f, &t, 0);
        }
    }

    #[test]
    fn depth_zero_is_a_leaf() {
        let a = atoms(&["p", "q"]);
        for seed in 0..20u32 {
            let f = gen_formula(&a, 0, &mut Mulberry32::new(seed));
            assert!(formalmethods::ast_size(&f) == 1);
        }
    }
}

mod atom_density_tests {
    use super::*;

    #[test]
    fn zero_density_forces_empty_states_and_falsifies_f_p() {
        let f = parse_ltl("F p").unwrap();
        let r = falsify(
            &f,
            &atoms(&["p", "q"]),
            &FalsifyOptions {
                seed: Some(1),
                iterations: Some(10),
                atom_density: Some(0.0),
                ..Default::default()
            },
        );
        assert!(r.violated, "all-empty traces must violate F p");
    }

    #[test]
    fn zero_density_never_falsifies_g_not_p() {
        let f = parse_ltl("G !p").unwrap();
        let r = falsify(
            &f,
            &atoms(&["p", "q"]),
            &FalsifyOptions {
                seed: Some(1),
                iterations: Some(10),
                atom_density: Some(0.0),
                ..Default::default()
            },
        );
        assert!(!r.violated, "all-empty traces satisfy G !p");
    }

    #[test]
    fn none_density_keeps_the_default_campaign() {
        // Parity guard: omitting the knob must be identical to the
        // pre-knob ½ campaign for the same seed.
        let f = parse_ltl("G (request -> F response)").unwrap();
        let a = atoms(&["request", "response", "timeout"]);
        let opts = |d| FalsifyOptions {
            seed: Some(424242),
            iterations: Some(50),
            length: Some(6),
            atom_density: d,
            ..Default::default()
        };
        let r1 = falsify(&f, &a, &opts(None));
        let r2 = falsify(&f, &a, &opts(Some(0.5)));
        assert_eq!(r1.tried, r2.tried);
        assert_eq!(r1.violated, r2.violated);
    }
}
