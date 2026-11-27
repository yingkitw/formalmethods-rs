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

    #[test]
    fn parses_the_past_time_operators_and_prints_them_back() {
        assert_eq!(print_ltl(&parse_ltl("Y p").unwrap()), "Y p");
        assert_eq!(print_ltl(&parse_ltl("O p").unwrap()), "O p");
        assert_eq!(print_ltl(&parse_ltl("H p").unwrap()), "H p");
        assert_eq!(print_ltl(&parse_ltl("p S q").unwrap()), "p S q");
        // shares level 5 with U/W/R, left-associative
        assert_eq!(print_ltl(&parse_ltl("p S q U r").unwrap()), "p S q U r");
        // unary args wrap, like X (F p)
        assert_eq!(print_ltl(&parse_ltl("X O p").unwrap()), "X (O p)");
    }

    #[test]
    fn past_operators_are_reserved_but_atoms_may_contain_their_letters() {
        assert!(parse_ltl("S").unwrap_err().to_string().contains("place it between"));
        assert!(parse_ltl("Y").unwrap_err().to_string().contains("end of formula"));
        assert!(parse_ltl("p U S").unwrap_err().to_string().contains("place it between"));
        let atoms: Vec<String> = collect_atoms(&parse_ltl("yes && Old && S2").unwrap())
            .into_iter()
            .collect();
        assert_eq!(atoms, ["Old", "S2", "yes"]);
    }
}

mod past_semantics {
    use super::*;

    #[test]
    fn y_looks_one_step_back_and_fails_at_zero() {
        let t = trace(&[&["p"], &[]]);
        assert!(!eval_at(&parse_ltl("Y p").unwrap(), &t, 0));
        assert!(eval_at(&parse_ltl("Y p").unwrap(), &t, 1));
        assert!(!eval_at(&parse_ltl("Y p").unwrap(), &trace(&[&["q"], &["p"]]), 1));
    }

    #[test]
    fn o_finds_a_witness_anywhere_in_the_past_including_now() {
        let t = trace(&[&["p"], &[], &[]]);
        assert!(eval_at(&parse_ltl("O p").unwrap(), &t, 0));
        assert!(eval_at(&parse_ltl("O p").unwrap(), &t, 2));
        assert!(!eval_at(&parse_ltl("O p").unwrap(), &trace(&[&[], &[]]), 1));
    }

    #[test]
    fn h_requires_the_atom_at_every_step_so_far() {
        assert!(eval_at(&parse_ltl("H p").unwrap(), &trace(&[&["p"], &["p"], &["p"]]), 2));
        assert!(!eval_at(&parse_ltl("H p").unwrap(), &trace(&[&["p"], &[]]), 1));
        // NOT vacuous like G: the past includes position 0 itself, where
        // the atom is false on an empty trace
        assert!(!satisfies_trace(&parse_ltl("H p").unwrap(), &trace(&[])));
    }

    #[test]
    fn s_needs_a_past_trigger_with_the_left_side_held_ever_since() {
        let t = trace(&[&["q"], &["p"], &["p"]]);
        assert!(eval_at(&parse_ltl("p S q").unwrap(), &t, 1));
        assert!(eval_at(&parse_ltl("p S q").unwrap(), &t, 2));
        // trigger right now
        assert!(eval_at(&parse_ltl("p S q").unwrap(), &trace(&[&["p"], &["q"]]), 1));
        // lapse after trigger
        assert!(!eval_at(&parse_ltl("p S q").unwrap(), &trace(&[&["q"], &["p"], &[]]), 2));
        // trigger never came
        assert!(!eval_at(&parse_ltl("p S q").unwrap(), &trace(&[&["p"], &["p"]]), 1));
    }

    #[test]
    fn past_ops_see_concrete_history_on_periodic_runs() {
        let pt = PeriodicTrace {
            prefix: trace(&[&["q"]]),
            cycle: trace(&[&[], &["p"]]), // 0=q, 1=∅, 2=p, 3=∅, 4=p…
        };
        assert!(eval_periodic_at(&parse_ltl("O q").unwrap(), &pt, 4)); // q lives in the prefix
        assert!(!eval_periodic_at(&parse_ltl("H p").unwrap(), &pt, 4));
        assert!(!eval_periodic_at(&parse_ltl("Y p").unwrap(), &pt, 2)); // index 1 was ∅
        assert!(eval_periodic_at(&parse_ltl("Y p").unwrap(), &pt, 3)); // index 2 was p
    }

    #[test]
    fn empty_cycle_reads_as_stuttering_the_last_prefix_state() {
        use formalmethods::periodic_atoms;
        let pt = PeriodicTrace {
            prefix: trace(&[&["a"], &["b"]]),
            cycle: vec![],
        };
        assert_eq!(periodic_atoms(&pt, 1), &trace(&[&["b"]])[0]);
        // beyond the end: stutters `b` — and must not panic
        assert_eq!(periodic_atoms(&pt, 5), &trace(&[&["b"]])[0]);
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

mod property_patterns {
    use super::*;
    use formalmethods::Formula;

    #[test]
    fn common_spec_shapes() {
        assert_eq!(Formula::always("ok").to_string(), "G ok");
        assert_eq!(Formula::eventually("done").to_string(), "F done");
        assert_eq!(
            Formula::responds_to("request", "response").to_string(),
            "G (request -> F response)"
        );
        // patterns compose with parsed formulas
        assert_eq!(
            Formula::always(Formula::responds_to("coin", "vend")).to_string(),
            "G (G (coin -> F vend))"
        );
    }

    #[test]
    fn patterns_evaluate_like_their_parse() {
        let built = Formula::responds_to("request", "response");
        let parsed = parse_ltl("G (request -> F response)").unwrap();
        assert_eq!(built, parsed);
        assert_eq!(
            satisfies_trace(&built, &trace(&[&["request"], &["response"]])),
            satisfies_trace(&parsed, &trace(&[&["request"], &["response"]]))
        );
    }
}

mod serde_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_shape_matches_the_ts_ast() {
        // The TypeScript engine serializes Formula as a tagged union:
        // {kind:"atom",name}, {kind:"not",arg}, {kind:"until",left,right}.
        assert_eq!(
            serde_json::to_value(parse_ltl("a").unwrap()).unwrap(),
            json!({"kind": "atom", "name": "a"})
        );
        assert_eq!(
            serde_json::to_value(parse_ltl("!a").unwrap()).unwrap(),
            json!({"kind": "not", "arg": {"kind": "atom", "name": "a"}})
        );
        assert_eq!(
            serde_json::to_value(parse_ltl("a U b").unwrap()).unwrap(),
            json!({
                "kind": "until",
                "left": {"kind": "atom", "name": "a"},
                "right": {"kind": "atom", "name": "b"},
            })
        );
        assert_eq!(
            serde_json::to_value(parse_ltl("G (coin -> F dispense)").unwrap()).unwrap(),
            json!({
                "kind": "globally",
                "arg": {
                    "kind": "implies",
                    "left": {"kind": "atom", "name": "coin"},
                    "right": {"kind": "finally", "arg": {"kind": "atom", "name": "dispense"}},
                },
            })
        );
        assert_eq!(
            serde_json::to_value(parse_ltl("true").unwrap()).unwrap(),
            json!({"kind": "true"})
        );
    }

    #[test]
    fn round_trip_every_variant() {
        for src in [
            "true",
            "false",
            "p",
            "!p",
            "a && b",
            "a || b",
            "a -> b",
            "a <-> b",
            "X p",
            "G p",
            "F p",
            "a U b",
            "a W b",
            "a R b",
            "G (request -> F (ack || X done)) W (timeout <-> done R abort)",
        ] {
            let f = parse_ltl(src).unwrap();
            let v = serde_json::to_value(&f).unwrap();
            let back: formalmethods::Formula = serde_json::from_value(v).unwrap();
            assert_eq!(back, f, "round-trip failed for {src}");
        }
    }

    #[test]
    fn rejects_unknown_kinds() {
        assert!(serde_json::from_value::<formalmethods::Formula>(
            json!({"kind": "tomorrow", "arg": {"kind": "atom", "name": "a"}})
        )
        .is_err());
        assert!(serde_json::from_value::<formalmethods::Formula>(json!({"kind": "atom"})).is_err());
    }
}
