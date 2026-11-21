//! `fm` — a tiny CLI front-end for the formalmethods engine.
//!
//! Each subcommand mirrors one capability of the library and takes a single
//! JSON argument (same shapes as the sibling project's MCP tools):
//!
//! ```sh
//! fm check_ltl        '{"formula":"G (request -> F response)","trace":[["request"],["response"]]}'
//! fm model_check      '{"formula":"G (coin -> F dispense)","model":{...}}'
//! fm falsify_property '{"formula":"G (a -> F b)","atoms":["a","b"],"seed":42}'
//! fm analyze_coverage '{"model":{...},"traces":[["idle","paid"]]}'
//! fm explain_concept  '{"concept":"ltl"}'      # or no arg for the index
//! ```
//!
//! Output is JSON on stdout; failures exit non-zero with the error on stderr.

use std::process::ExitCode;

use formalmethods::{
    analyze_coverage, falsify, get_concept, concept_index, model_check, parse_kripke, parse_ltl,
    print_ltl, satisfies_trace, truth_table, FalsifyOptions, ModelCheckOptions, State, Trace,
};
use serde_json::{Value, json};

const USAGE: &str = "\
fm — formal methods for testing

usage: fm <tool> [json-args]

tools:
  check_ltl         evaluate an LTL formula on a finite trace (LTLf)
  model_check       model check a formula against a Kripke structure
  falsify_property  random-trace falsification with shrinking
  analyze_coverage  state/transition coverage of test traces
  explain_concept   the field guide (no arg → index of topics)
";

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

fn args_json(args: &[String]) -> Result<Value, String> {
    match args {
        [] => Ok(json!({})),
        [one] => serde_json::from_str(one).map_err(|e| format!("invalid JSON args: {e}")),
        _ => Err("expected at most one JSON argument".into()),
    }
}

fn req<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key)
        .ok_or_else(|| format!("missing required `{key}`"))
}

fn as_str<'a>(v: &'a Value, what: &str) -> Result<&'a str, String> {
    v.as_str().ok_or_else(|| format!("`{what}` must be a string"))
}

fn as_u64(v: &Value, what: &str) -> Result<u64, String> {
    v.as_u64()
        .ok_or_else(|| format!("`{what}` must be a non-negative integer"))
}

fn as_usize(v: Option<&Value>, what: &str) -> Result<Option<usize>, String> {
    v.map(|x| as_u64(x, what).map(|n| n as usize)).transpose()
}

fn as_string_vec(v: &Value, what: &str) -> Result<Vec<String>, String> {
    v.as_array()
        .ok_or_else(|| format!("`{what}` must be an array of strings"))?
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("`{what}` must be an array of strings"))
        })
        .collect()
}

fn as_trace(v: &Value, what: &str) -> Result<Trace, String> {
    v.as_array()
        .ok_or_else(|| format!("`{what}` must be an array of states"))?
        .iter()
        .map(|s| as_string_vec(s, "trace state").map(|v| v.into_iter().collect::<State>()))
        .collect()
}

fn as_formula(v: &Value) -> Result<formalmethods::Formula, String> {
    parse_ltl(as_str(v, "formula")?).map_err(|e| format!("invalid formula: {e}"))
}

fn emit(v: &impl serde::Serialize) -> ExitCode {
    match serde_json::to_string_pretty(v) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(format!("could not serialize result: {e}")),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(tool) = argv.first() else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };
    if matches!(tool.as_str(), "-h" | "--help" | "help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let args = match args_json(&argv[1..]) {
        Ok(v) => v,
        Err(e) => return fail(e),
    };

    let run = || -> Result<Value, String> {
        match tool.as_str() {
            "check_ltl" => {
                let formula = as_formula(req(&args, "formula")?)?;
                let trace = as_trace(req(&args, "trace")?, "trace")?;
                Ok(json!({
                    "semantics": "finite-trace (LTLf)",
                    "formula": print_ltl(&formula),
                    "satisfies": satisfies_trace(&formula, &trace),
                    "truth_table": truth_table(&formula, &trace),
                }))
            }
            "model_check" => {
                let formula = as_formula(req(&args, "formula")?)?;
                let (model, warnings) =
                    parse_kripke(req(&args, "model")?).map_err(|e| format!("invalid model: {e}"))?;
                let result = model_check(
                    &formula,
                    &model,
                    &ModelCheckOptions {
                        depth: as_usize(args.get("depth"), "depth")?,
                        path_budget: as_usize(args.get("path_budget"), "path_budget")?,
                        max_counterexamples: as_usize(
                            args.get("max_counterexamples"),
                            "max_counterexamples",
                        )?,
                    },
                );
                let mut out = serde_json::to_value(&result).map_err(|e| e.to_string())?;
                let obj = out.as_object_mut().unwrap();
                obj.insert("semantics".into(), "infinite-run (lasso) — dead ends stutter".into());
                obj.insert("formula".into(), print_ltl(&formula).into());
                obj.insert("model_warnings".into(), json!(warnings));
                Ok(out)
            }
            "falsify_property" => {
                let formula = as_formula(req(&args, "formula")?)?;
                let atoms = as_string_vec(req(&args, "atoms")?, "atoms")?;
                if atoms.is_empty() {
                    return Err("`atoms` must be non-empty (the traces need an alphabet)".into());
                }
                let result = falsify(
                    &formula,
                    &atoms,
                    &FalsifyOptions {
                        iterations: as_usize(args.get("iterations"), "iterations")?,
                        length: as_usize(args.get("trace_length"), "trace_length")?,
                        seed: args.get("seed").map(|v| {
                            as_u64(v, "seed").map(|n| n as u32)
                        }).transpose()?,
                    },
                );
                Ok(json!({
                    "semantics": "finite-trace (LTLf), random generation + shrinking",
                    "formula": print_ltl(&formula),
                    "violated": result.violated,
                    "tried": result.tried,
                    "iterations": result.iterations,
                    "trace_length": result.length,
                    "seed": result.seed,
                    "shrink_steps": result.shrink_steps.unwrap_or(0),
                    "counterexample": result.counterexample,
                    "original": result.original,
                }))
            }
            "analyze_coverage" => {
                let (model, _warnings) =
                    parse_kripke(req(&args, "model")?).map_err(|e| format!("invalid model: {e}"))?;
                let traces = req(&args, "traces")?
                    .as_array()
                    .ok_or("`traces` must be an array of traces")?
                    .iter()
                    .map(|t| as_string_vec(t, "traces[i]"))
                    .collect::<Result<Vec<Vec<String>>, _>>()?;
                serde_json::to_value(analyze_coverage(&model, &traces))
                    .map_err(|e| e.to_string())
            }
            "explain_concept" => match args.get("concept") {
                None => Ok(json!({
                    "topics": concept_index().iter().map(|(id, title, tagline)| json!({
                        "id": id, "title": title, "tagline": tagline,
                    })).collect::<Vec<_>>(),
                    "hint": "Call again with `concept: <id>` for the full explanation.",
                })),
                Some(v) => {
                    let id = as_str(v, "concept")?;
                    let c = get_concept(id).ok_or_else(|| {
                        let avail = concept_index()
                            .iter()
                            .map(|(id, _, _)| *id)
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("unknown concept '{id}'. Available: {avail}")
                    })?;
                    Ok(json!({
                        "id": c.id,
                        "title": c.title,
                        "tagline": c.tagline,
                        "sections": c.sections.iter().map(|s| json!({
                            "heading": s.heading,
                            "paragraphs": s.paragraphs,
                            "bullets": s.bullets,
                        })).collect::<Vec<_>>(),
                        "related": c.related,
                    }))
                }
            },
            other => Err(format!("unknown tool '{other}'\n\n{USAGE}")),
        }
    };

    match run() {
        Ok(v) => emit(&v),
        Err(e) => fail(e),
    }
}
