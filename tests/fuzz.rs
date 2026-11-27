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
