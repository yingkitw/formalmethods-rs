//! `fm` — a tiny CLI front-end for the formalmethods engine.
//!
//! Each subcommand mirrors one capability of the library and takes a single
//! JSON argument (same shapes as the sibling project's MCP tools):
//!
//! ```sh
//! fm check_ltl        '{"formula":"G (request -> F response)","trace":[["request"],["response"]]}'
//! fm monitor_ltl      '{"formula":"G (a -> F b)","trace":[["a"],["b"]]}'
//! fm model_check      '{"formula":"G (coin -> F dispense)","model":{...}}'
//! fm ctl_check        '{"formula":"AG (coin -> AF vend)","model":{...}}'
//! fm analyze_depth    '{"formula":"G (coin -> F dispense)","model":{...}}'
//! fm falsify_property '{"formula":"G (a -> F b)","atoms":["a","b"],"seed":42,"guided":true}'
//! fm analyze_coverage '{"model":{...},"traces":[["idle","paid"]]}'
//! fm analyze_spec_coverage '{"formula":"G (coin -> F dispense)","model":{...}}'
//! fm explain_concept  '{"concept":"ltl"}'      # or no arg for the index
//! ```
//!
//! Output is JSON on stdout; failures exit non-zero with the error on stderr.

use std::process::ExitCode;

use formalmethods::{
    analyze_coverage, analyze_depth, analyze_spec_coverage, counterexample_run, ctl_check,
    falsify, falsifying_window, get_concept, concept_index, model_check, monitor_trace,
    mutation_report, parse_ctl, parse_kripke, parse_ltl, presets, print_ctl, print_ltl,
    satisfies_trace, truth_table, validate_kripke, FalsifyOptions, KripkeSpec,
    ModelCheckOptions, State, Trace,
};
use serde_json::{Value, json};

const USAGE: &str = "\
fm — formal methods for testing

usage: fm <tool> [json-args]
       json-args is a single JSON object; pass '-' to read it from stdin

tools:
  check_ltl         evaluate an LTL formula on a finite trace (LTLf)
  monitor_ltl       monitor a formula state-by-state, residual obligation per step
  model_check       model check a formula against a Kripke structure
  ctl_check         check a CTL formula (EF/AG/E[.. U ..]/...) over the state graph
  analyze_depth     the same model check re-run at every prefix depth
  mutation_report   spec-coverage: mutate the model, re-check the formula suite
  analyze_spec_coverage  mutate the formula's subformulas, re-check the model
  falsify_property  random-trace falsification with shrinking (guided=true for coverage-guided)
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
        [one] => {
            let src = if one == "-" {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
                    .map_err(|e| format!("could not read stdin: {e}"))?;
                buf
            } else {
                one.clone()
            };
            serde_json::from_str(&src).map_err(|e| format!("invalid JSON args: {e}"))
        }
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

/// `model` object or `preset` name — every model-taking tool accepts
/// either, like the MCP server.
fn resolve_model(args: &Value) -> Result<(KripkeSpec, Vec<String>), String> {
    if let Some(p) = args.get("preset") {
        let name = p
            .as_str()
            .ok_or_else(|| "`preset` must be a string".to_string())?;
        let mut spec = presets::preset(name).ok_or_else(|| {
            format!(
                "unknown preset \"{name}\" — expected one of: {}",
                presets::preset_names().join(", ")
            )
        })?;
        let warnings = validate_kripke(&mut spec).map_err(|e| e.to_string())?;
        return Ok((spec, warnings));
    }
    let v = req(args, "model").map_err(|_| "missing required `model` (or pass `preset` instead)".to_string())?;
    parse_kripke(v).map_err(|e| format!("invalid model: {e}"))
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
                let (model, warnings) = resolve_model(&args)?;
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
                // Counterexamples in the MCP shape: the lasso split plus a
                // finite replay (null when the violation needs infinity).
                let ces: Vec<Value> = result
                    .counterexamples
                    .iter()
                    .map(|ce| {
                        let run = counterexample_run(&model, ce);
                        json!({
                            "path": ce.path,
                            "cycle_start": ce.cycle_start,
                            "prefix": &ce.path[..ce.cycle_start],
                            "cycle": &ce.path[ce.cycle_start..],
                            "replay": falsifying_window(&formula, &run, &[]),
                            "truth_table": ce.window.table,
                        })
                    })
                    .collect();
                obj.insert("counterexamples".into(), json!(ces));
                Ok(out)
            }
            "monitor_ltl" => {
                let formula = as_formula(req(&args, "formula")?)?;
                let trace = as_trace(req(&args, "trace")?, "trace")?;
                let run = monitor_trace(&formula, &trace);
                Ok(json!({
                    "semantics": "finite-trace (LTLf), monitor by formula progression",
                    "formula": run.formula,
                    "satisfies": run.verdict,
                    "steps": run.steps,
                    "notes": if trace.is_empty() {
                        json!(["empty trace — no states consumed; verdict is the vacuous answer on zero data"])
                    } else {
                        json!([])
                    },
                }))
            }
            "analyze_depth" => {
                let formula = as_formula(req(&args, "formula")?)?;
                let (model, warnings) = resolve_model(&args)?;
                let sweep = analyze_depth(
                    &formula,
                    &model,
                    &ModelCheckOptions {
                        path_budget: as_usize(args.get("path_budget"), "path_budget")?,
                        ..Default::default()
                    },
                );
                let mut out = serde_json::to_value(&sweep).map_err(|e| e.to_string())?;
                let obj = out.as_object_mut().unwrap();
                obj.insert(
                    "semantics".into(),
                    "infinite-run (lasso) — the same check re-run at every prefix depth".into(),
                );
                obj.insert("formula".into(), print_ltl(&formula).into());
                obj.insert("model_warnings".into(), json!(warnings));
                Ok(out)
            }
            "analyze_spec_coverage" => {
                let formula = as_formula(req(&args, "formula")?)?;
                let (model, warnings) = resolve_model(&args)?;
                let report = analyze_spec_coverage(
                    &formula,
                    &model,
                    &ModelCheckOptions {
                        depth: as_usize(args.get("depth"), "depth")?,
                        path_budget: as_usize(args.get("path_budget"), "path_budget")?,
                        ..Default::default()
                    },
                    as_usize(args.get("max_mutants"), "max_mutants")?,
                );
                let rows: Vec<Value> = report
                    .mutants
                    .iter()
                    .map(|m| {
                        json!({
                            "flipped": m.target,
                            "kind": m.kind,
                            "mutation": m.mutation,
                            "verdict": m.verdict,
                            "status": m.status,
                            "witness": m.witness,
                        })
                    })
                    .collect();
                Ok(json!({
                    "semantics": "infinite-run (lasso) — each subformula mutated (negation / strengthen / operand-drop), model re-checked",
                    "formula": print_ltl(&formula),
                    "base_verdict": report.base_verdict,
                    "score": report.score,
                    "killed": report.killed,
                    "survived": report.survived,
                    "unknown": report.unknown,
                    "total": report.total,
                    "mutants": rows,
                    "model_warnings": warnings,
                    "notes": report.notes,
                }))
            }
            "ctl_check" => {
                let formula = parse_ctl(
                    req(&args, "formula")?
                        .as_str()
                        .ok_or("`formula` must be a string")?,
                )
                .map_err(|e| format!("CTL parse error: {e}"))?;
                let (model, warnings) = resolve_model(&args)?;
                let result =
                    ctl_check(&model, &formula).map_err(|e| format!("invalid model: {e}"))?;
                let mut out = serde_json::to_value(&result).map_err(|e| e.to_string())?;
                let obj = out.as_object_mut().unwrap();
                obj.insert("semantics".into(), "branching-time (CTL) — dead ends stutter".into());
                obj.insert("formula".into(), print_ctl(&formula).into());
                obj.insert("model_warnings".into(), json!(warnings));
                Ok(out)
            }
            "mutation_report" => {
                let (model, warnings) = resolve_model(&args)?;
                let formulas = req(&args, "formulas")?
                    .as_array()
                    .ok_or("`formulas` must be an array of formula strings")?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| "each formula must be a string".to_string())
                            .and_then(|s| parse_ltl(s).map_err(|e| format!("`{s}`: {e}")))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out =
                    serde_json::to_value(mutation_report(&model, &formulas)).map_err(|e| e.to_string())?;
                let obj = out.as_object_mut().unwrap();
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
                        guided: args.get("guided").and_then(|v| v.as_bool()),
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
                    "guided": result.coverage.is_some(),
                    "shrink_steps": result.shrink_steps.unwrap_or(0),
                    "counterexample": result.counterexample,
                    "original": result.original,
                    "coverage": result.coverage,
                }))
            }
            "analyze_coverage" => {
                let (model, _warnings) = resolve_model(&args)?;
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
