use formalmethods::{Monitor, MonitorVerdict, parse_ltl, satisfies_trace, trace};

fn run(src: &str, t: formalmethods::Trace) -> (MonitorVerdict, bool) {
    let mut m = Monitor::new(&parse_ltl(src).unwrap());
    let mut v = MonitorVerdict::Pending;
    for s in &t {
        v = m.feed(s);
    }
    let _ = v;
    (m.verdict(), m.close())
}

mod agreement {
    use super::*;

    /// The monitor's final verdict must equal `satisfies_trace` on the
    /// consumed log — the same property, two execution modes.
    #[test]
    fn close_agrees_with_satisfies_trace() {
        let cases: Vec<(&str, formalmethods::Trace)> = vec![
            ("G (request -> F response)", trace(&[&["request"], &["response"]])),
            ("G (request -> F response)", trace(&[&["request"], &["timeout"]])),
            ("F done", trace(&[&["busy"], &["done"]])),
            ("F done", trace(&[&["busy"], &["busy"]])),
            ("G ok", trace(&[&["ok"], &["ok"]])),
            ("G ok", trace(&[&["ok"], &[]])),
            ("a U b", trace(&[&["a"], &["a"], &["b"]])),
            ("a U b", trace(&[&["a"], &["a"]])),
            ("a W b", trace(&[&["a"], &["a"]])),
            ("a W b", trace(&[&["a"], &[], &[]])),
            ("b R a", trace(&[&["a"], &["a", "b"]])),
            ("X a", trace(&[&["a"]])),
            ("X a", trace(&[&[], &["a"]])),
            ("G (a -> X a)", trace(&[&["a"], &["a"]])),
            ("G (a -> X a)", trace(&[&["a"], &[]])),
            ("a <-> b", trace(&[&["a", "b"], &[]])),
            ("a <-> b", trace(&[&["a"], &[]])),
            ("F (a || !a)", trace(&[&[], &[]])),
            // The X anchor: `X φ` owes a position that must exist.
            ("X !a", trace(&[&[]])),          // violated — no next state
            ("X !a", trace(&[&[], &["b"]])),  // satisfied — next lacks a
            ("!X a", trace(&[&[]])),          // satisfied — !(no next) is true
            ("F X !a", trace(&[&[]])),        // violated
            ("X X a", trace(&[&[], &["a"]])), // violated — pos 2 missing
            // Past operators evaluate against the consumed history.
            ("O a", trace(&[&[], &["a"], &[]])),
            ("H a", trace(&[&["a"], &["a"]])),
            ("H a", trace(&[&["a"], &[]])),
            ("Y a", trace(&[&["a"], &[]])),
            ("a S b", trace(&[&["b"], &["a"], &["a"]])),
            ("a S b", trace(&[&["b"], &["a"], &[]])),
        ];
        for (src, t) in cases {
            let (_, verdict) = run(src, t.clone());
            let f = parse_ltl(src).unwrap();
            assert_eq!(
                verdict,
                satisfies_trace(&f, &t),
                "{src} on {t:?}: monitor disagrees with satisfies_trace"
            );
        }
    }

    #[test]
    fn empty_stream_is_vacuous_for_g_and_w() {
        let m = Monitor::new(&parse_ltl("G a").unwrap());
        assert!(m.close());
        let m = Monitor::new(&parse_ltl("a W b").unwrap());
        assert!(m.close());
        let m = Monitor::new(&parse_ltl("F a").unwrap());
        assert!(!m.close());
        let m = Monitor::new(&parse_ltl("X a").unwrap());
        assert!(!m.close());
    }
}

mod streaming {
    use super::*;

    #[test]
    fn decides_early_once_a_liveness_witness_arrives() {
        let mut m = Monitor::new(&parse_ltl("F done").unwrap());
        assert_eq!(m.feed(&["busy"].into_iter().map(String::from).collect()), MonitorVerdict::Pending);
        assert_eq!(m.feed(&["done"].into_iter().map(String::from).collect()), MonitorVerdict::Satisfied);
        // decided stays decided — no suffix can undo it
        assert_eq!(m.feed(&Default::default()), MonitorVerdict::Satisfied);
        assert!(m.close());
    }

    #[test]
    fn violation_is_sticky() {
        let mut m = Monitor::new(&parse_ltl("G ok").unwrap());
        assert_eq!(m.feed(&["ok"].into_iter().map(String::from).collect()), MonitorVerdict::Pending);
        assert_eq!(m.feed(&Default::default()), MonitorVerdict::Violated);
        assert_eq!(m.feed(&["ok"].into_iter().map(String::from).collect()), MonitorVerdict::Violated);
        assert!(!m.close());
    }

    #[test]
    fn residual_exposes_the_pending_obligation() {
        let mut m = Monitor::new(&parse_ltl("F b").unwrap());
        m.feed(&Default::default());
        assert!(!m.is_decided());
        // the obligation is readable: "something must still show b"
        assert_eq!(m.residual().to_string(), "F b");
    }

    #[test]
    fn x_residual_keeps_its_anchor() {
        let mut m = Monitor::new(&parse_ltl("X a").unwrap());
        m.feed(&Default::default());
        // "a at the next state, and a next state must exist"
        assert_eq!(m.residual().to_string(), "a && F true");
    }
}

mod batch {
    use formalmethods::monitor::{MonitorVerdict, monitor_trace};

    use super::*;

    #[test]
    fn one_residual_row_per_state() {
        let run = monitor_trace(
            &parse_ltl("G (request -> F response)").unwrap(),
            &trace(&[&["request"], &["response"]]),
        );
        assert_eq!(run.steps.len(), 2);
        assert_eq!(run.steps[0].status, MonitorVerdict::Pending);
        // `F response` owed now; `G` keeps the unrolled original formula
        assert_eq!(run.steps[0].residual, "F response && G (request -> F response)");
        assert_eq!(run.steps[1].status, MonitorVerdict::Satisfied);
        assert!(run.verdict);
    }

    #[test]
    fn verdict_matches_satisfies_trace() {
        for (src, t) in [
            ("G ok", trace(&[&["ok"], &[]])),
            ("F done", trace(&[&[], &["done"]])),
            ("X !a", trace(&[&[]])),
            ("O a", trace(&[&[], &["a"]])),
        ] {
            let f = parse_ltl(src).unwrap();
            assert_eq!(
                monitor_trace(&f, &t).verdict,
                satisfies_trace(&f, &t),
                "{src}"
            );
        }
    }
}
