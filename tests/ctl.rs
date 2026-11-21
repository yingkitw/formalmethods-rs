#![cfg(feature = "std")]

use formalmethods::{
    CtlFormula, KripkeSpec, Verdict, ctl_check, parse_ctl, parse_kripke, print_ctl,
};
use serde_json::json;

fn spec(v: serde_json::Value) -> KripkeSpec {
    let (spec, _warnings) = parse_kripke(&v).unwrap();
    spec
}

/// idle --coin--> paid --> vend --> idle   (the good vending machine)
fn vending() -> KripkeSpec {
    spec(json!({
        "atoms": ["coin", "paid", "vend"],
        "initial": ["idle"],
        "states": [
            { "id": "idle", "atoms": [], "next": ["idle", "paid"] },
            { "id": "paid", "atoms": ["coin", "paid"], "next": ["vend"] },
            { "id": "vend", "atoms": ["vend"], "next": ["idle"] }
        ]
    }))
}

fn check(s: &KripkeSpec, src: &str) -> Verdict {
    ctl_check(s, &parse_ctl(src).unwrap()).unwrap().verdict
}

mod parser {
    use super::*;

    #[test]
    fn parses_path_quantifiers() {
        assert_eq!(parse_ctl("EF vend").unwrap(), CtlFormula::Ef(Box::new(CtlFormula::Atom("vend".into()))));
        assert!(matches!(parse_ctl("E[coin U vend]").unwrap(), CtlFormula::Eu(..)));
        assert!(matches!(parse_ctl("A[coin U vend]").unwrap(), CtlFormula::Au(..)));
    }

    #[test]
    fn precedence_and_parens() {
        // `->` needs parens under a unary quantifier; `&&` inside a binary
        // does not.
        assert_eq!(
            print_ctl(&parse_ctl("AG (coin -> EF vend)").unwrap()),
            "AG (coin -> EF vend)"
        );
        assert_eq!(
            print_ctl(&parse_ctl("EF vend && AX paid").unwrap()),
            "EF vend && AX paid"
        );
    }

    #[test]
    fn round_trip() {
        for src in ["EF vend", "AG (coin -> AF vend)", "E[paid U vend] && AX true", "!(EG vend)"] {
            assert_eq!(parse_ctl(&print_ctl(&parse_ctl(src).unwrap())), parse_ctl(src));
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_ctl("").is_err());
        assert!(parse_ctl("E[coin vend]").is_err());
        assert!(parse_ctl("EX").is_err());
    }
}

mod checker {
    use super::*;

    #[test]
    fn ef_ag() {
        assert_eq!(check(&vending(), "EF vend"), Verdict::Holds);
        assert_eq!(check(&vending(), "AG (paid -> AF vend)"), Verdict::Holds);
        assert_eq!(check(&vending(), "AG paid"), Verdict::Violated);
    }

    #[test]
    fn ex_ax() {
        // idle has a self-loop and a paid edge.
        assert_eq!(check(&vending(), "EX paid"), Verdict::Holds);  // idle can go to paid
        assert_eq!(check(&vending(), "AX paid"), Verdict::Violated); // but can also stay idle
        // successors of idle are {idle, paid}: `paid` labels paid only,
        // `!coin` covers idle.
        assert_eq!(check(&vending(), "AX (!coin || paid)"), Verdict::Holds);
    }

    #[test]
    fn eu_au() {
        assert_eq!(check(&vending(), "E[true U vend]"), Verdict::Holds);
        assert_eq!(check(&vending(), "A[true U vend]"), Verdict::Violated); // idle self-loop starves vend
        assert_eq!(check(&vending(), "A[paid U vend]"), Verdict::Violated); // fails at idle (paid false, vend false)
        assert_eq!(check(&vending(), "E[true U paid]"), Verdict::Holds);
    }

    #[test]
    fn eg_finds_the_self_loop() {
        // idle can loop forever without paying — EG !vend holds globally.
        assert_eq!(check(&vending(), "EG !vend"), Verdict::Holds);
        // but vend is not sustainable forever.
        assert_eq!(check(&vending(), "EG vend"), Verdict::Violated);
    }

    #[test]
    fn dead_end_stutters() {
        // A dead end acts as a self-loop: AX/AF see the same state again.
        let s = spec(json!({
            "atoms": ["done"],
            "initial": ["start"],
            "states": [
                { "id": "start", "atoms": [], "next": ["end"] },
                { "id": "end", "atoms": ["done"], "next": [] }
            ]
        }));
        assert_eq!(check(&s, "AF done"), Verdict::Holds);
        // At `end` there are no successors — AX is evaluated through the
        // stutter loop, not vacuously: AX done holds because done is true.
        let r = ctl_check(&s, &parse_ctl("EX done").unwrap()).unwrap();
        assert_eq!(r.verdict, Verdict::Holds);
        // EG done: the stutter loop sustains it at `end`, reachable.
        assert_eq!(check(&s, "EG done"), Verdict::Violated); // start can't reach it while holding done
    }

    #[test]
    fn violating_initials_reported() {
        let s = spec(json!({
            "atoms": ["p"],
            "initial": ["good", "bad"],
            "states": [
                { "id": "good", "atoms": ["p"], "next": ["good"] },
                { "id": "bad", "atoms": [], "next": ["bad"] }
            ]
        }));
        let r = ctl_check(&s, &parse_ctl("AG p").unwrap()).unwrap();
        assert_eq!(r.verdict, Verdict::Violated);
        assert_eq!(r.violating_initials, vec!["bad"]);
        assert_eq!(r.satisfying, vec!["good"]);
    }

    #[test]
    fn undeclared_atom_satisfies_nothing() {
        assert_eq!(check(&vending(), "EF nonexistent"), Verdict::Violated);
    }

    #[test]
    fn invalid_spec_is_an_error() {
        // Build programmatically — parse_kripke validates on the way in,
        // so an invalid spec must reach ctl_check via the data API.
        let s = KripkeSpec {
            atoms: ["p"].into_iter().map(String::from).collect(),
            initial: vec!["ghost".into()],
            states: vec![formalmethods::KripkeState {
                id: "s".into(),
                atoms: ["p"].into_iter().map(String::from).collect(),
                next: vec!["s".into()],
            }],
        };
        assert!(ctl_check(&s, &parse_ctl("EF p").unwrap()).is_err());
    }
}
