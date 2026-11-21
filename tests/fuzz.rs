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
