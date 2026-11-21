use formalmethods::{
    PeriodicTrace, collect_atoms, eval_at, eval_periodic_at, parse_ltl, periodic_truth_table,
    print_ltl, satisfies_trace, trace, truth_table,
};

mod parser {
    use super::*;

    #[test]
    fn parses_the_full_grammar_and_prints_it_back() {
        assert_eq!(print_ltl(&parse_ltl("G (a -> F b)").unwrap()), "G (a -> F b)");
        assert_eq!(print_ltl(&parse_ltl("X ready").unwrap()), "X ready");
        // U binds tighter than ||
        assert_eq!(print_ltl(&parse_ltl("a U b || c").unwrap()), "a U b || c");
        // U binds tighter than &&
        assert_eq!(print_ltl(&parse_ltl("!a U b && c").unwrap()), "!a U b && c");
        assert_eq!(print_ltl(&parse_ltl("a <-> !a").unwrap()), "a <-> !a");
        // W and R share precedence, round-trips flat
        assert_eq!(print_ltl(&parse_ltl("(a W b) R c").unwrap()), "a W b R c");
    }

    #[test]
    fn collects_atoms() {
        let atoms: Vec<String> = collect_atoms(&parse_ltl("G (req -> F ack)").unwrap())
            .into_iter()
            .collect();
        assert_eq!(atoms, ["ack", "req"]);
    }

    #[test]
    fn rejects_nonsense_with_a_position() {
        assert!(parse_ltl("G (").unwrap_err().to_string().contains("end of formula"));
        assert!(parse_ltl("a @ b").unwrap_err().to_string().contains("offset"));
        assert!(parse_ltl("").is_err());
    }
}

mod truth_tables {
    use super::*;

    #[test]
    fn bad_positions_are_where_the_formula_reads_false() {
        let cases: Vec<(&str, formalmethods::Trace)> = vec![
            ("G (coin -> F dispense)", trace(&[&["coin"], &["timeout"], &["timeout"]])),
            ("F done", trace(&[&["busy"], &["busy"]])),
            ("G ok", trace(&[&["ok"], &["ok"]])),
            ("a U b", trace(&[&["a"], &["a"]])),
            ("X a", trace(&[&["a"]])),
        ];
        for (src, tr) in cases {
            let t = truth_table(&parse_ltl(src).unwrap(), &tr);
            let last = t.columns.len() - 1;
            // consistency: a position is bad iff the formula column reads F there
            for (i, row) in t.rows.iter().enumerate() {
                assert_eq!(t.bad_positions.contains(&i), !row.values[last], "{src} at {i}");
            }
            // the verdict is exactly "position 0 is bad"
            assert_eq!(t.bad_positions.contains(&0), !t.result);
        }
    }

    #[test]
    fn periodic_same_property_on_lassos() {
        let pts: Vec<(&str, PeriodicTrace)> = vec![
            (
                "G (coin -> F dispense)",
                PeriodicTrace {
                    prefix: vec![],
                    cycle: trace(&[&["coin"], &["timeout"]]),
                },
            ),
            (
                "G F go",
                PeriodicTrace {
                    prefix: trace(&[&["stop"]]),
                    cycle: trace(&[&["stop"]]),
                },
            ),
            (
                "G a",
                PeriodicTrace {
                    prefix: vec![],
                    cycle: trace(&[&["a"]]),
                },
            ),
        ];
        for (src, pt) in pts {
            let t = periodic_truth_table(&parse_ltl(src).unwrap(), &pt, 6);
            let last = t.columns.len() - 1;
            assert_eq!(t.bad_positions.contains(&0), !t.result);
            for (i, row) in t.rows.iter().enumerate() {
                assert_eq!(t.bad_positions.contains(&i), !row.values[last], "{src} at {i}");
            }
        }
    }
}

mod finite_trace_semantics {
    use super::*;

    #[test]
    fn x_is_false_at_the_last_position() {
        assert!(!satisfies_trace(&parse_ltl("X a").unwrap(), &trace(&[&["a"]])));
        assert!(satisfies_trace(&parse_ltl("X a").unwrap(), &trace(&[&[], &["a"]])));
    }

    #[test]
    fn f_and_g_quantify_over_the_observed_trace_only() {
        assert!(satisfies_trace(&parse_ltl("F done").unwrap(), &trace(&[&["busy"], &["done"]])));
        assert!(!satisfies_trace(&parse_ltl("F done").unwrap(), &trace(&[&["busy"], &["busy"]])));
        assert!(satisfies_trace(&parse_ltl("G ok").unwrap(), &trace(&[&["ok"], &["ok"]])));
        assert!(!satisfies_trace(&parse_ltl("G ok").unwrap(), &trace(&[&["ok"], &[]])));
    }

    #[test]
    fn until_demands_the_right_side_happens() {
        assert!(satisfies_trace(&parse_ltl("a U b").unwrap(), &trace(&[&["a"], &["a"], &["b"]])));
        assert!(!satisfies_trace(&parse_ltl("a U b").unwrap(), &trace(&[&["a"], &["a"]])));
        assert!(!satisfies_trace(&parse_ltl("a U b").unwrap(), &trace(&[&[], &["b"]])));
    }

    #[test]
    fn weak_until_forgives_release_is_its_dual() {
        assert!(satisfies_trace(&parse_ltl("a W b").unwrap(), &trace(&[&["a"], &["a"]])));
        assert!(!satisfies_trace(&parse_ltl("a W b").unwrap(), &trace(&[&["a"], &[], &[]])));
        assert!(satisfies_trace(&parse_ltl("b R a").unwrap(), &trace(&[&["a"], &["a", "b"]])));
    }
}

mod periodic_semantics {
    use super::*;

    fn pt() -> PeriodicTrace {
        PeriodicTrace {
            prefix: trace(&[&["a"]]),
            cycle: trace(&[&[], &["b"]]),
        }
    }

    #[test]
    fn f_and_g_see_through_the_cycle() {
        assert!(eval_periodic_at(&parse_ltl("F b").unwrap(), &pt(), 0));
        assert!(!eval_periodic_at(&parse_ltl("G a").unwrap(), &pt(), 0));
        assert!(eval_periodic_at(
            &parse_ltl("G (a -> X a)").unwrap(),
            &PeriodicTrace {
                prefix: vec![],
                cycle: trace(&[&["a"]]),
            },
            0
        ));
    }

    #[test]
    fn x_wraps_around_the_cycle() {
        assert!(eval_periodic_at(
            &parse_ltl("X b").unwrap(),
            &PeriodicTrace {
                prefix: vec![],
                cycle: trace(&[&["a"], &["b"]]),
            },
            0
        ));
        assert!(eval_periodic_at(
            &parse_ltl("X X b").unwrap(),
            &PeriodicTrace {
                prefix: vec![],
                cycle: trace(&[&["b"], &["a"]]),
            },
            0
        ));
    }

    #[test]
    fn agrees_with_the_finite_evaluator_on_a_finite_prefix() {
        let tr = trace(&[&["a"], &["b"]]);
        assert_eq!(
            eval_periodic_at(
                &parse_ltl("a U b").unwrap(),
                &PeriodicTrace {
                    prefix: tr.clone(),
                    cycle: vec![],
                },
                0
            ),
            eval_at(&parse_ltl("a U b").unwrap(), &tr, 0)
        );
    }
}
