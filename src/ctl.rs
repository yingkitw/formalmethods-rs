//! Branching-time (CTL) checking over [`KripkeSpec`] structures.
//!
//! Where `model` checks linear-time (LTL) properties over individual runs,
//! `ctl` checks *branching-time* properties over the state graph itself:
//! `EF p` ("some reachable state satisfies p"), `AG p` ("p is an
//! invariant"), `E[p U q]`, `AX p`, and friends.
//!
//! Checking is exact on finite structures — there is no `incomplete`
//! verdict — and uses the classic fixpoint labelling algorithm.
//!
//! **Dead ends stutter**, matching the LTL checker: a state with no
//! outgoing edges is treated as having an implicit self-loop, so `AX p`
//! at a dead end is equivalent to `p` there (never vacuous-true).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;

use crate::model::{KripkeError, KripkeSpec, Verdict, validate_kripke};

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

/// A CTL formula. State subformulas sit under path quantifiers
/// (`E`xists-path / `A`ll-paths) composed with `X`/`F`/`G`/`U`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtlFormula {
    True,
    False,
    Atom(String),
    Not(Box<CtlFormula>),
    And(Box<CtlFormula>, Box<CtlFormula>),
    Or(Box<CtlFormula>, Box<CtlFormula>),
    Implies(Box<CtlFormula>, Box<CtlFormula>),
    Iff(Box<CtlFormula>, Box<CtlFormula>),
    /// `EX f` — some immediate successor satisfies `f`.
    Ex(Box<CtlFormula>),
    /// `AX f` — every immediate successor satisfies `f`.
    Ax(Box<CtlFormula>),
    /// `EF f` — some reachable state satisfies `f`.
    Ef(Box<CtlFormula>),
    /// `AF f` — every path eventually reaches `f`.
    Af(Box<CtlFormula>),
    /// `EG f` — some path keeps `f` true forever.
    Eg(Box<CtlFormula>),
    /// `AG f` — `f` is an invariant over all reachable states.
    Ag(Box<CtlFormula>),
    /// `E[l U r]` — some path satisfies `l` until `r` (which must occur).
    Eu(Box<CtlFormula>, Box<CtlFormula>),
    /// `A[l U r]` — every path satisfies `l` until `r` (which must occur).
    Au(Box<CtlFormula>, Box<CtlFormula>),
}

impl fmt::Display for CtlFormula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&print_ctl(self))
    }
}

/// Print a CTL formula in the grammar `parse_ctl` accepts.
pub fn print_ctl(f: &CtlFormula) -> String {
    match f {
        CtlFormula::True => "true".into(),
        CtlFormula::False => "false".into(),
        CtlFormula::Atom(a) => a.clone(),
        CtlFormula::Not(a) => format!("!{}", unary_arg(a)),
        CtlFormula::And(l, r) => format!("{} && {}", bin_arg(l, 4), bin_arg(r, 4)),
        CtlFormula::Or(l, r) => format!("{} || {}", bin_arg(l, 3), bin_arg(r, 3)),
        CtlFormula::Implies(l, r) => format!("{} -> {}", bin_arg(l, 2), bin_arg(r, 2)),
        CtlFormula::Iff(l, r) => format!("{} <-> {}", bin_arg(l, 1), bin_arg(r, 1)),
        CtlFormula::Ex(a) => format!("EX {}", unary_arg(a)),
        CtlFormula::Ax(a) => format!("AX {}", unary_arg(a)),
        CtlFormula::Ef(a) => format!("EF {}", unary_arg(a)),
        CtlFormula::Af(a) => format!("AF {}", unary_arg(a)),
        CtlFormula::Eg(a) => format!("EG {}", unary_arg(a)),
        CtlFormula::Ag(a) => format!("AG {}", unary_arg(a)),
        CtlFormula::Eu(l, r) => format!("E[{} U {}]", print_ctl(l), print_ctl(r)),
        CtlFormula::Au(l, r) => format!("A[{} U {}]", print_ctl(l), print_ctl(r)),
    }
}

fn prec(f: &CtlFormula) -> u8 {
    match f {
        CtlFormula::Iff(..) => 1,
        CtlFormula::Implies(..) => 2,
        CtlFormula::Or(..) => 3,
        CtlFormula::And(..) => 4,
        _ => 5,
    }
}

fn bin_arg(f: &CtlFormula, parent: u8) -> String {
    let s = print_ctl(f);
    if prec(f) < parent { format!("({s})") } else { s }
}

fn unary_arg(f: &CtlFormula) -> String {
    let s = print_ctl(f);
    if prec(f) < 5 { format!("({s})") } else { s }
}

// ---------------------------------------------------------------------------
// Parser — same conventions as the LTL grammar (SPEC.md): `!` `&&` `||`
// `->` `<->`, `E[.. U ..]` / `A[.. U ..]`, EX/AX/EF/AF/EG/AG prefix.
// ---------------------------------------------------------------------------

/// An error produced by [`parse_ctl`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtlParseError(pub String);

impl fmt::Display for CtlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CtlParseError {}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Id(String),
    True,
    False,
    Not,
    And,
    Or,
    Imp,
    Iff,
    LParen,
    RParen,
    LBracket,
    RBracket,
    U,
    E,
    A,
    Ex,
    Ax,
    Ef,
    Af,
    Eg,
    Ag,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, CtlParseError> {
    let mut toks = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let t = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            '!' => Tok::Not,
            '&' => {
                if chars.get(i + 1) == Some(&'&') {
                    i += 1;
                }
                Tok::And
            }
            '|' => {
                if chars.get(i + 1) == Some(&'|') {
                    i += 1;
                }
                Tok::Or
            }
            '-' if chars.get(i + 1) == Some(&'>') => {
                i += 1;
                Tok::Imp
            }
            '<' if chars.get(i + 1) == Some(&'-') && chars.get(i + 2) == Some(&'>') => {
                i += 2;
                Tok::Iff
            }
            _ if c.is_ascii_alphanumeric() || c == '_' => {
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let w: String = chars[i..j].iter().collect();
                i = j;
                match w.as_str() {
                    "true" | "TRUE" => Tok::True,
                    "false" | "FALSE" => Tok::False,
                    "EX" => Tok::Ex,
                    "AX" => Tok::Ax,
                    "EF" => Tok::Ef,
                    "AF" => Tok::Af,
                    "EG" => Tok::Eg,
                    "AG" => Tok::Ag,
                    "E" => Tok::E,
                    "A" => Tok::A,
                    "U" => Tok::U,
                    _ => Tok::Id(w),
                }
            }
            _ => return Err(CtlParseError(format!("unexpected character '{c}' at {i}"))),
        };
        match t {
            Tok::LParen | Tok::RParen | Tok::LBracket | Tok::RBracket
            | Tok::Not | Tok::And | Tok::Or | Tok::Imp | Tok::Iff => i += 1,
            _ => {}
        }
        toks.push(t);
    }
    Ok(toks)
}

struct P<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.pos);
        self.pos += 1;
        t
    }
    fn expect(&mut self, t: &Tok) -> Result<(), CtlParseError> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(CtlParseError(format!(
                "expected {t:?}, found {:?}",
                self.peek()
            )))
        }
    }

    fn iff(&mut self) -> Result<CtlFormula, CtlParseError> {
        let mut l = self.imp()?;
        while self.peek() == Some(&Tok::Iff) {
            self.pos += 1;
            let r = self.imp()?;
            l = CtlFormula::Iff(Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn imp(&mut self) -> Result<CtlFormula, CtlParseError> {
        let l = self.or()?;
        if self.peek() == Some(&Tok::Imp) {
            self.pos += 1;
            let r = self.imp()?;
            return Ok(CtlFormula::Implies(Box::new(l), Box::new(r)));
        }
        Ok(l)
    }
    fn or(&mut self) -> Result<CtlFormula, CtlParseError> {
        let mut l = self.and()?;
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            let r = self.and()?;
            l = CtlFormula::Or(Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<CtlFormula, CtlParseError> {
        let mut l = self.until_or_unary()?;
        while self.peek() == Some(&Tok::And) {
            self.pos += 1;
            let r = self.until_or_unary()?;
            l = CtlFormula::And(Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn until_or_unary(&mut self) -> Result<CtlFormula, CtlParseError> {
        match self.peek() {
            Some(&Tok::E) | Some(&Tok::A) => {
                let is_e = *self.peek().unwrap() == Tok::E;
                self.pos += 1;
                self.expect(&Tok::LBracket)?;
                let l = self.iff()?;
                self.expect(&Tok::U)?;
                let r = self.iff()?;
                self.expect(&Tok::RBracket)?;
                Ok(if is_e {
                    CtlFormula::Eu(Box::new(l), Box::new(r))
                } else {
                    CtlFormula::Au(Box::new(l), Box::new(r))
                })
            }
            _ => self.unary(),
        }
    }
    fn unary(&mut self) -> Result<CtlFormula, CtlParseError> {
        match self.peek() {
            Some(&Tok::Not) => {
                self.pos += 1;
                Ok(CtlFormula::Not(Box::new(self.unary()?)))
            }
            Some(&Tok::Ex) => {
                self.pos += 1;
                Ok(CtlFormula::Ex(Box::new(self.unary()?)))
            }
            Some(&Tok::Ax) => {
                self.pos += 1;
                Ok(CtlFormula::Ax(Box::new(self.unary()?)))
            }
            Some(&Tok::Ef) => {
                self.pos += 1;
                Ok(CtlFormula::Ef(Box::new(self.unary()?)))
            }
            Some(&Tok::Af) => {
                self.pos += 1;
                Ok(CtlFormula::Af(Box::new(self.unary()?)))
            }
            Some(&Tok::Eg) => {
                self.pos += 1;
                Ok(CtlFormula::Eg(Box::new(self.unary()?)))
            }
            Some(&Tok::Ag) => {
                self.pos += 1;
                Ok(CtlFormula::Ag(Box::new(self.unary()?)))
            }
            Some(&Tok::LParen) => {
                self.pos += 1;
                let f = self.iff()?;
                self.expect(&Tok::RParen)?;
                Ok(f)
            }
            Some(&Tok::True) => {
                self.pos += 1;
                Ok(CtlFormula::True)
            }
            Some(&Tok::False) => {
                self.pos += 1;
                Ok(CtlFormula::False)
            }
            Some(&Tok::Id(_)) => {
                let Tok::Id(w) = self.next().unwrap() else { unreachable!() };
                Ok(CtlFormula::Atom(w.clone()))
            }
            other => Err(CtlParseError(format!("expected a formula, found {other:?}"))),
        }
    }
}

/// Parse a CTL formula.
///
/// Grammar (loosest first): `<->` → `->` → `||` → `&&` →
/// `E[.. U ..]` / `A[.. U ..]` → `!`/`EX`/`AX`/`EF`/`AF`/`EG`/`AG` →
/// parens / atoms / `true` / `false`. The words `E`, `A`, `U`, `EX`, `AX`,
/// `EF`, `AF`, `EG`, `AG` are reserved.
pub fn parse_ctl(src: &str) -> Result<CtlFormula, CtlParseError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err(CtlParseError("empty formula".into()));
    }
    let mut p = P { toks: &toks, pos: 0 };
    let f = p.iff()?;
    if p.pos != toks.len() {
        return Err(CtlParseError(format!(
            "trailing tokens after formula at {}",
            p.pos
        )));
    }
    Ok(f)
}

// ---------------------------------------------------------------------------
// Checker — fixpoint labelling over the state graph
// ---------------------------------------------------------------------------

/// The result of a CTL check. `verdict` is `holds` iff every initial state
/// satisfies the formula; CTL checking is exact on finite structures, so
/// the result is never `incomplete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CtlResult {
    pub verdict: Verdict,
    /// State ids where the formula holds.
    pub satisfying: Vec<String>,
    /// Initial states where it fails (empty iff `holds`).
    pub violating_initials: Vec<String>,
}

/// Check a CTL formula against a Kripke structure.
///
/// Validates the spec first (same rules as `model_check`). An atom not
/// declared in the model simply satisfies no state.
pub fn ctl_check(spec: &KripkeSpec, formula: &CtlFormula) -> Result<CtlResult, KripkeError> {
    let mut spec = spec.clone();
    validate_kripke(&mut spec)?;
    let ids: Vec<&str> = spec.states.iter().map(|s| s.id.as_str()).collect();
    let index: BTreeMap<&str, usize> = ids.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    // Successors with the dead-end stuttering convention: no out-edges →
    // implicit self-loop, so a dead end satisfies AX φ iff it satisfies φ.
    let succ: Vec<Vec<usize>> = spec
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let edges: Vec<usize> = s
                .next
                .iter()
                .filter_map(|t| index.get(t.as_str()).copied())
                .collect();
            if edges.is_empty() { vec![i] } else { edges }
        })
        .collect();
    let atoms: Vec<&BTreeSet<String>> = spec.states.iter().map(|s| &s.atoms).collect();
    let sat = sat_set(formula, &atoms, &succ);
    let satisfying: Vec<String> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| sat[*i])
        .map(|(_, s)| (*s).to_string())
        .collect();
    let violating_initials: Vec<String> = spec
        .initial
        .iter()
        .filter(|id| index.get(id.as_str()).is_some_and(|i| !sat[*i]))
        .cloned()
        .collect();
    Ok(CtlResult {
        verdict: if violating_initials.is_empty() {
            Verdict::Holds
        } else {
            Verdict::Violated
        },
        satisfying,
        violating_initials,
    })
}

fn sat_set(f: &CtlFormula, atoms: &[&BTreeSet<String>], succ: &[Vec<usize>]) -> Vec<bool> {
    let n = atoms.len();
    match f {
        CtlFormula::True => vec![true; n],
        CtlFormula::False => vec![false; n],
        CtlFormula::Atom(a) => atoms.iter().map(|s| s.contains(a)).collect(),
        CtlFormula::Not(a) => sat_set(a, atoms, succ).iter().map(|b| !b).collect(),
        CtlFormula::And(l, r) => {
            let (l, r) = (sat_set(l, atoms, succ), sat_set(r, atoms, succ));
            (0..n).map(|i| l[i] && r[i]).collect()
        }
        CtlFormula::Or(l, r) => {
            let (l, r) = (sat_set(l, atoms, succ), sat_set(r, atoms, succ));
            (0..n).map(|i| l[i] || r[i]).collect()
        }
        CtlFormula::Implies(l, r) => {
            let (l, r) = (sat_set(l, atoms, succ), sat_set(r, atoms, succ));
            (0..n).map(|i| !l[i] || r[i]).collect()
        }
        CtlFormula::Iff(l, r) => {
            let (l, r) = (sat_set(l, atoms, succ), sat_set(r, atoms, succ));
            (0..n).map(|i| l[i] == r[i]).collect()
        }
        CtlFormula::Ex(a) => {
            let s = sat_set(a, atoms, succ);
            (0..n).map(|i| succ[i].iter().any(|&j| s[j])).collect()
        }
        CtlFormula::Ax(a) => {
            let s = sat_set(a, atoms, succ);
            (0..n).map(|i| succ[i].iter().all(|&j| s[j])).collect()
        }
        CtlFormula::Ef(a) => sat_set(
            &CtlFormula::Eu(Box::new(CtlFormula::True), a.clone()),
            atoms,
            succ,
        ),
        CtlFormula::Af(a) => sat_set(
            &CtlFormula::Au(Box::new(CtlFormula::True), a.clone()),
            atoms,
            succ,
        ),
        CtlFormula::Eg(a) => {
            // Greatest fixpoint: start from all φ-states, drop any that
            // can't keep φ alive through some successor.
            let mut w = sat_set(a, atoms, succ);
            loop {
                let mut changed = false;
                for i in 0..n {
                    if w[i] && !succ[i].iter().any(|&j| w[j]) {
                        w[i] = false;
                        changed = true;
                    }
                }
                if !changed {
                    return w;
                }
            }
        }
        CtlFormula::Ag(a) => {
            // AG φ ≡ ¬EF ¬φ.
            let not = sat_set(
                &CtlFormula::Eu(
                    Box::new(CtlFormula::True),
                    Box::new(CtlFormula::Not(a.clone())),
                ),
                atoms,
                succ,
            );
            not.iter().map(|b| !b).collect()
        }
        CtlFormula::Eu(l, r) => {
            // Least fixpoint: W = sat(r) ∪ (sat(l) ∩ EX W).
            let (sl, mut w) = (sat_set(l, atoms, succ), sat_set(r, atoms, succ));
            loop {
                let mut changed = false;
                for i in 0..n {
                    if !w[i] && sl[i] && succ[i].iter().any(|&j| w[j]) {
                        w[i] = true;
                        changed = true;
                    }
                }
                if !changed {
                    return w;
                }
            }
        }
        CtlFormula::Au(l, r) => {
            // Least fixpoint: W = sat(r) ∪ (sat(l) ∩ AX W).
            let (sl, mut w) = (sat_set(l, atoms, succ), sat_set(r, atoms, succ));
            loop {
                let mut changed = false;
                for i in 0..n {
                    if !w[i] && sl[i] && succ[i].iter().all(|&j| w[j]) {
                        w[i] = true;
                        changed = true;
                    }
                }
                if !changed {
                    return w;
                }
            }
        }
    }
}
