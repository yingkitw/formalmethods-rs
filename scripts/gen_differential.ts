// Generates the differential fixture: a deterministic corpus of
// (formula, input, expected-verdict) triples computed by the TS engine.
// Regenerate:
//   cd ../formalmethods-rs && bun scripts/gen_differential.ts > tests/fixtures/differential.json
//
// Requires the sibling TS project at ../formalmethods.

import { parseLtl, evalAt, satisfiesPeriodic, printLtl } from "../../formalmethods/src/lib/formal/ltl";
import { modelCheck } from "../../formalmethods/src/lib/formal/model";
import { mulberry32 } from "../../formalmethods/src/lib/formal/fuzz";

const rng = mulberry32(0x5eed);
const ATOMS = ["p", "q", "r"];

// randltl-style formula generator: uniform over node kinds, leaves at
// depth 0. Mirrors the shape of gen_formula in the Rust engine.
type Formula = any;
function genFormula(depth: number): Formula {
  const leaf = () => (rng() < 0.8 ? { kind: "atom", name: ATOMS[(rng() * 3) | 0] } : { kind: rng() < 0.5 ? "true" : "false" });
  if (depth === 0) return leaf();
  const d = depth - 1;
  const sub = () => genFormula(d);
  const pick = (rng() * 18) | 0;
  switch (pick) {
    case 0: case 1: case 2: return leaf();
    case 3: return { kind: "not", arg: sub() };
    case 4: return { kind: "next", arg: sub() };
    case 5: return { kind: "globally", arg: sub() };
    case 6: return { kind: "finally", arg: sub() };
    case 7: return { kind: "yesterday", arg: sub() };
    case 8: return { kind: "once", arg: sub() };
    case 9: return { kind: "historically", arg: sub() };
    case 10: return { kind: "and", left: sub(), right: sub() };
    case 11: return { kind: "or", left: sub(), right: sub() };
    case 12: return { kind: "implies", left: sub(), right: sub() };
    case 13: return { kind: "iff", left: sub(), right: sub() };
    case 14: return { kind: "until", left: sub(), right: sub() };
    case 15: return { kind: "weak", left: sub(), right: sub() };
    case 16: return { kind: "release", left: sub(), right: sub() };
    default: return { kind: "since", left: sub(), right: sub() };
  }
}

function genTrace(): string[][] {
  const len = (rng() * 8) | 0; // 0..7 — the empty trace included
  const t: string[][] = [];
  for (let i = 0; i < len; i++) {
    const s: string[] = [];
    for (const a of ATOMS) if (rng() < 0.5) s.push(a);
    t.push(s);
  }
  return t;
}

function genLasso(): { prefix: string[][]; cycle: string[][] } {
  const state = () => ATOMS.filter(() => rng() < 0.5);
  const prefix = Array.from({ length: (rng() * 4) | 0 }, state);
  const cycle = Array.from({ length: 1 + ((rng() * 4) | 0) }, state);
  return { prefix, cycle };
}

function genSpec(n: number) {
  const ids = Array.from({ length: n }, (_, i) => `s${i}`);
  const states = ids.map((id) => ({
    id,
    atoms: ATOMS.filter(() => rng() < 0.4),
    next: [...new Set(ids.filter(() => rng() < 0.35))],
  }));
  return { atoms: ATOMS, initial: [ids[0]], states };
}

const FORMULAS = Array.from({ length: 100 }, (_, i) => genFormula(2 + (i % 3)));
const TRACES = Array.from({ length: 20 }, genTrace);
TRACES.push([], [[]], [["p", "q", "r"], ["p", "q", "r"], ["p", "q", "r"]]);
const LASSOS = Array.from({ length: 12 }, genLasso);
const MODELS = Array.from({ length: 15 }, (_, i) => genSpec(3 + (i % 3)));

const evalRows: [number, number, number][] = [];
const periodicRows: [number, number, number][] = [];
const checkRows: [number, number, string][] = [];

for (let f = 0; f < FORMULAS.length; f++) {
  const formula = FORMULAS[f];
  for (let t = 0; t < TRACES.length; t++) {
    evalRows.push([f, t, evalAt(formula, TRACES[t], 0) ? 1 : 0]);
  }
  for (let l = 0; l < LASSOS.length; l++) {
    periodicRows.push([f, l, satisfiesPeriodic(formula, LASSOS[l]) ? 1 : 0]);
  }
  for (let m = 0; m < MODELS.length; m++) {
    const r = modelCheck(formula, MODELS[m]);
    checkRows.push([f, m, r.verdict === "holds" ? "h" : r.verdict === "violated" ? "v" : "i"]);
  }
}

console.log(JSON.stringify({
  formulas: FORMULAS.map((f) => ({ src: printLtl(f), ast: f })),
  traces: TRACES,
  lassos: LASSOS,
  models: MODELS,
  eval: evalRows,
  periodic: periodicRows,
  check: checkRows,
}));
