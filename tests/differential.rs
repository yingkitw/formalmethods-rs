//! Differential test: replay a fixture of (input, verdict) triples computed
//! by the TS engine (`scripts/gen_differential.ts`) and assert the Rust
//! engine agrees on every row. Direct parity evidence — not mirrored test
//! structure, but the sibling engine's actual output.
#![cfg(feature = "std")]

use formalmethods::{
    Formula, KripkeSpec, KripkeState, ModelCheckOptions, PeriodicTrace, State, Trace,
    Verdict, eval_at, model_check, parse_ltl, satisfies_periodic,
};
use serde_json::Value;

static FIXTURE: &str = include_str!("fixtures/differential.json");

fn load() -> Value {
    serde_json::from_str(FIXTURE).expect("fixture parses")
}

fn to_trace(v: &Value) -> Trace {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            s.as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect::<State>()
        })
        .collect()
}

fn to_spec(v: &Value) -> KripkeSpec {
    KripkeSpec {
        atoms: v["atoms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect(),
        initial: v["initial"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect(),
        states: v["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| KripkeState {
                id: s["id"].as_str().unwrap().to_string(),
                atoms: s["atoms"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect(),
                next: s["next"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect(),
            })
            .collect(),
    }
}

/// Deserialize the formula ASTs emitted by the TS engine — itself a parity
/// check on the tagged-JSON serde — and confirm our parser accepts every
/// `printLtl` string the TS engine produced.
fn formulas(fx: &Value) -> Vec<Formula> {
    fx["formulas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rec| {
            let f: Formula = serde_json::from_value(rec["ast"].clone())
                .unwrap_or_else(|e| panic!("AST {} did not deserialize: {e}", rec["src"]));
            parse_ltl(rec["src"].as_str().unwrap())
                .unwrap_or_else(|e| panic!("printed src {} did not parse: {e}", rec["src"]));
            f
        })
        .collect()
}

fn mismatches(label: &str, bad: Vec<String>) {
    assert!(
        bad.is_empty(),
        "{label}: {} mismatches, first: {}",
        bad.len(),
        bad.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")
    );
}

#[test]
fn ltl_eval_verdicts_match_the_ts_engine() {
    let fx = load();
    let formulas = formulas(&fx);
    let traces: Vec<Trace> = fx["traces"].as_array().unwrap().iter().map(to_trace).collect();
    let mut bad = Vec::new();
    for row in fx["eval"].as_array().unwrap() {
        let (f, t) = (row[0].as_u64().unwrap() as usize, row[1].as_u64().unwrap() as usize);
        let expected = row[2].as_u64().unwrap() == 1;
        let got = eval_at(&formulas[f], &traces[t], 0);
        if got != expected {
            bad.push(format!(
                "eval {:?} on {:?}: ts={expected} rs={got}",
                formalmethods::print_ltl(&formulas[f]),
                traces[t]
            ));
        }
    }
    mismatches("eval", bad);
}

#[test]
fn periodic_verdicts_match_the_ts_engine() {
    let fx = load();
    let formulas = formulas(&fx);
    let lassos: Vec<PeriodicTrace> = fx["lassos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| PeriodicTrace {
            prefix: to_trace(&l["prefix"]),
            cycle: to_trace(&l["cycle"]),
        })
        .collect();
    let mut bad = Vec::new();
    for row in fx["periodic"].as_array().unwrap() {
        let (f, l) = (row[0].as_u64().unwrap() as usize, row[1].as_u64().unwrap() as usize);
        let expected = row[2].as_u64().unwrap() == 1;
        let got = satisfies_periodic(&formulas[f], &lassos[l]);
        if got != expected {
            bad.push(format!(
                "periodic {} on lasso {l}: ts={expected} rs={got}",
                formalmethods::print_ltl(&formulas[f])
            ));
        }
    }
    mismatches("periodic", bad);
}

#[test]
fn model_check_verdicts_match_the_ts_engine() {
    let fx = load();
    let formulas = formulas(&fx);
    let models: Vec<KripkeSpec> = fx["models"].as_array().unwrap().iter().map(to_spec).collect();
    let mut bad = Vec::new();
    for row in fx["check"].as_array().unwrap() {
        let (f, m) = (row[0].as_u64().unwrap() as usize, row[1].as_u64().unwrap() as usize);
        let expected = match row[2].as_str().unwrap() {
            "h" => Verdict::Holds,
            "v" => Verdict::Violated,
            _ => Verdict::Incomplete,
        };
        let got = model_check(&formulas[f], &models[m], &ModelCheckOptions::default()).verdict;
        if got != expected {
            bad.push(format!(
                "model_check {} on model {m}: ts={expected} rs={got}",
                formalmethods::print_ltl(&formulas[f])
            ));
        }
    }
    mismatches("model_check", bad);
}
