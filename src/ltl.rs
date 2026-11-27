//! LTL syntax, parsing, and evaluation.
//!
//! Two semantics live side by side in this crate:
//!
//! - **Finite-trace semantics (LTLf)** — evaluate a formula over a *finished*
//!   execution such as a test case or a log. `X` at the last position is
//!   false, `F`/`G` quantify over the observed trace only.
//!   Used by [`satisfies_trace`] and the fuzzer.
//! - **Ultimately-periodic semantics** — evaluate over an infinite run that is
//!   eventually periodic (prefix + cycle). Used by the model checker: every
//!   finite Kripke structure realises such runs, and every LTL violation
//!   that exists at all is witnessed by one of them.

use core::fmt;

#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
#[cfg(feature = "std")]
use std::collections::{BTreeMap, BTreeSet};

/// An LTL formula.
///
/// Binary operators box both sides as `(left, right)`; for [`Until`],
/// [`Weak`], [`Release`], and [`Since`] the tuple is `(left, right)`.
/// Past-time operators (`Yesterday`, `Once`, `Historically`, `Since`)
/// look backward over the consumed prefix — handy for checking logs.
///
/// [`Until`]: Formula::Until
/// [`Weak`]: Formula::Weak
/// [`Release`]: Formula::Release
/// [`Since`]: Formula::Since
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    True,
    False,
    Atom(String),
    Not(Box<Formula>),
    And(Box<Formula>, Box<Formula>),
    Or(Box<Formula>, Box<Formula>),
    Implies(Box<Formula>, Box<Formula>),
    Iff(Box<Formula>, Box<Formula>),
    Next(Box<Formula>),
    Globally(Box<Formula>),
    Finally(Box<Formula>),
    /// `Y` — the argument held at the previous position (false at 0).
    Yesterday(Box<Formula>),
    /// `O` — the argument held at some past-or-present position.
    Once(Box<Formula>),
    /// `H` — the argument held at every past-or-present position.
    Historically(Box<Formula>),
    Until(Box<Formula>, Box<Formula>),
    Weak(Box<Formula>, Box<Formula>),
    Release(Box<Formula>, Box<Formula>),
    /// `l S r` — `r` held at some past-or-present position and `l` held
    /// at every position strictly since then.
    Since(Box<Formula>, Box<Formula>),
}

impl Formula {
    /// Convenience constructor for [`Formula::Atom`].
    pub fn atom(name: impl Into<String>) -> Self {
        Formula::Atom(name.into())
    }

    /// Parse a formula from source text. Alias for [`parse_ltl`].
    pub fn parse(src: &str) -> Result<Self, LtlParseError> {
        parse_ltl(src)
    }

    /// `G p` — safety: `p` holds in every state.
    pub fn always(p: impl Into<Formula>) -> Self {
        Formula::Globally(Box::new(p.into()))
    }

    /// `F p` — liveness: `p` holds somewhere later.
    pub fn eventually(p: impl Into<Formula>) -> Self {
        Formula::Finally(Box::new(p.into()))
    }

    /// `G (req -> F res)` — the request/response contract: every `req` is
    /// eventually answered by `res`.
    pub fn responds_to(req: impl Into<Formula>, res: impl Into<Formula>) -> Self {
        Formula::Globally(Box::new(Formula::Implies(
            Box::new(req.into()),
            Box::new(Formula::eventually(res)),
        )))
    }
}

impl From<&str> for Formula {
    fn from(name: &str) -> Self {
        Formula::atom(name)
    }
}

impl From<String> for Formula {
    fn from(name: String) -> Self {
        Formula::Atom(name)
    }
}

/// A state is the set of atomic propositions holding there.
pub type State = BTreeSet<String>;

/// A trace is a sequence of states.
pub type Trace = Vec<State>;

/// Build a [`Trace`] from slices of atom names — handy in tests and examples.
///
/// ```
/// use formalmethods::trace;
/// let t = trace(&[&["a"], &["a", "b"], &[]]);
/// assert_eq!(t.len(), 3);
/// ```
pub fn trace(states: &[&[&str]]) -> Trace {
    states
        .iter()
        .map(|s| s.iter().map(|a| a.to_string()).collect())
        .collect()
}

/// An infinite run: `prefix` followed by `cycle` repeated forever.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeriodicTrace {
    pub prefix: Trace,
    pub cycle: Trace,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

const RESERVED: &[&str] = &[
    "X", "G", "F", "U", "W", "R", "Y", "O", "H", "S", "true", "false",
];
const UNARY_OPS: &[&str] = &["X", "G", "F", "Y", "O", "H"];

/// An error produced while parsing an LTL formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LtlParseError {
    pub message: String,
    /// Byte offset into the source where the error occurred, if known.
    pub position: Option<usize>,
}

impl LtlParseError {
    fn new(message: impl Into<String>, position: Option<usize>) -> Self {
        LtlParseError {
            message: message.into(),
            position,
        }
    }
}

impl fmt::Display for LtlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some(pos) => write!(f, "{} (at offset {})", self.message, pos),
            None => f.write_str(&self.message),
        }
    }
}

impl core::error::Error for LtlParseError {}

struct Tok {
    t: String,
    pos: usize,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, LtlParseError> {
    let bytes = src.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' || c == ')' || c == '!' {
            toks.push(Tok {
                t: c.to_string(),
                pos: i,
            });
            i += 1;
            continue;
        }
        if c == '&' || c == '|' {
            let t = format!("{c}{c}");
            i += if i + 1 < bytes.len() && bytes[i + 1] == c as u8 {
                2
            } else {
                1
            };
            toks.push(Tok { t, pos: i });
            continue;
        }
        if src[i..].starts_with("<->") {
            toks.push(Tok {
                t: "<->".into(),
                pos: i,
            });
            i += 3;
            continue;
        }
        if src[i..].starts_with("->") {
            toks.push(Tok {
                t: "->".into(),
                pos: i,
            });
            i += 2;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i;
            while j < bytes.len() {
                let d = bytes[j] as char;
                if !(d.is_ascii_alphanumeric() || d == '_') {
                    break;
                }
                j += 1;
            }
            toks.push(Tok {
                t: src[i..j].to_string(),
                pos: i,
            });
            i = j;
            continue;
        }
        return Err(LtlParseError::new(
            format!("Unexpected character '{c}'"),
            Some(i),
        ));
    }
    Ok(toks)
}

struct Parser {
    toks: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn parse(&mut self) -> Result<Formula, LtlParseError> {
        if self.toks.is_empty() {
            return Err(LtlParseError::new("Empty formula", None));
        }
        let f = self.parse_iff()?;
        if self.i < self.toks.len() {
            let tok = &self.toks[self.i];
            return Err(LtlParseError::new(
                format!("Unexpected '{}'", tok.t),
                Some(tok.pos),
            ));
        }
        Ok(f)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }

    fn take(&mut self) -> Result<&Tok, LtlParseError> {
        let tok = self
            .toks
            .get(self.i)
            .ok_or_else(|| LtlParseError::new("Unexpected end of formula", None))?;
        self.i += 1;
        Ok(tok)
    }

    fn at(&self, ts: &[&str]) -> bool {
        self.peek().is_some_and(|t| ts.contains(&t.t.as_str()))
    }

    fn expect(&mut self, t: &str) -> Result<(), LtlParseError> {
        let tok = self.take()?;
        if tok.t != t {
            return Err(LtlParseError::new(
                format!("Expected '{t}', found '{}'", tok.t),
                Some(tok.pos),
            ));
        }
        Ok(())
    }

    fn parse_iff(&mut self) -> Result<Formula, LtlParseError> {
        let mut left = self.parse_implies()?;
        while self.at(&["<->"]) {
            self.take()?;
            left = Formula::Iff(Box::new(left), Box::new(self.parse_implies()?));
        }
        Ok(left)
    }

    fn parse_implies(&mut self) -> Result<Formula, LtlParseError> {
        let left = self.parse_or()?;
        if self.at(&["->"]) {
            self.take()?;
            return Ok(Formula::Implies(
                Box::new(left),
                Box::new(self.parse_implies()?),
            ));
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Formula, LtlParseError> {
        let mut left = self.parse_and()?;
        while self.at(&["||"]) {
            self.take()?;
            left = Formula::Or(Box::new(left), Box::new(self.parse_and()?));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Formula, LtlParseError> {
        let mut left = self.parse_until()?;
        while self.at(&["&&"]) {
            self.take()?;
            left = Formula::And(Box::new(left), Box::new(self.parse_until()?));
        }
        Ok(left)
    }

    fn parse_until(&mut self) -> Result<Formula, LtlParseError> {
        let mut left = self.parse_unary()?;
        while self.at(&["U", "W", "R", "S"]) {
            let op = self.take()?.t.clone();
            let right = self.parse_unary()?;
            left = match op.as_str() {
                "U" => Formula::Until(Box::new(left), Box::new(right)),
                "W" => Formula::Weak(Box::new(left), Box::new(right)),
                "R" => Formula::Release(Box::new(left), Box::new(right)),
                _ => Formula::Since(Box::new(left), Box::new(right)),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Formula, LtlParseError> {
        let tok = self
            .peek()
            .ok_or_else(|| LtlParseError::new("Unexpected end of formula", None))?;
        match tok.t.as_str() {
            "!" => {
                self.take()?;
                Ok(Formula::Not(Box::new(self.parse_unary()?)))
            }
            "X" => {
                self.take()?;
                Ok(Formula::Next(Box::new(self.parse_unary()?)))
            }
            "G" => {
                self.take()?;
                Ok(Formula::Globally(Box::new(self.parse_unary()?)))
            }
            "F" => {
                self.take()?;
                Ok(Formula::Finally(Box::new(self.parse_unary()?)))
            }
            "Y" => {
                self.take()?;
                Ok(Formula::Yesterday(Box::new(self.parse_unary()?)))
            }
            "O" => {
                self.take()?;
                Ok(Formula::Once(Box::new(self.parse_unary()?)))
            }
            "H" => {
                self.take()?;
                Ok(Formula::Historically(Box::new(self.parse_unary()?)))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<Formula, LtlParseError> {
        let tok = self.take()?;
        if tok.t == "(" {
            let f = self.parse_iff()?;
            self.expect(")")?;
            return Ok(f);
        }
        match tok.t.as_str() {
            "true" => return Ok(Formula::True),
            "false" => return Ok(Formula::False),
            t if RESERVED.contains(&t) => {
                return Err(LtlParseError::new(
                    if UNARY_OPS.contains(&t) {
                        format!(
                            "'{t}' is a prefix LTL operator — put it before a formula (e.g. \"{t} a\")"
                        )
                    } else {
                        format!(
                            "'{t}' is an LTL operator — place it between formulas (e.g. \"a {t} b\")"
                        )
                    },
                    Some(tok.pos),
                ));
            }
            _ => {}
        }
        Ok(Formula::Atom(tok.t.clone()))
    }
}

/// Parse an LTL formula.
///
/// Grammar (loosest to tightest binding): `<->`, `->`, `||`, `&&`,
/// `U`/`W`/`R`/`S`, unary `!`/`X`/`G`/`F`/`Y`/`O`/`H`, atoms, `true`,
/// `false`, parenthesised formulas.
pub fn parse_ltl(src: &str) -> Result<Formula, LtlParseError> {
    let mut parser = Parser {
        toks: tokenize(src.trim())?,
        i: 0,
    };
    parser.parse()
}

// ---------------------------------------------------------------------------
// Printing & inspection
// ---------------------------------------------------------------------------

fn precedence(f: &Formula) -> u8 {
    match f {
        Formula::Iff(..) => 1,
        Formula::Implies(..) => 2,
        Formula::Or(..) => 3,
        Formula::And(..) => 4,
        Formula::Until(..) | Formula::Weak(..) | Formula::Release(..) | Formula::Since(..) => 5,
        Formula::Not(..)
        | Formula::Next(..)
        | Formula::Globally(..)
        | Formula::Finally(..)
        | Formula::Yesterday(..)
        | Formula::Once(..)
        | Formula::Historically(..) => 6,
        Formula::Atom(_) | Formula::True | Formula::False => 9,
    }
}

/// Print a formula in the same surface syntax the parser accepts.
///
/// The output round-trips exactly: `parse_ltl(&print_ltl(f)) == f`.
/// Equal-precedence children are parenthesised on the side the parser
/// would re-associate away from — e.g. right children of the
/// left-associative `U`/`W`/`R`/`S` chain, left children of the
/// right-associative `->`.
pub fn print_ltl(f: &Formula) -> String {
    fn rec(f: &Formula, parent: u8) -> String {
        let prec = precedence(f);
        let wrap = |s: String| if prec < parent { format!("({s})") } else { s };
        match f {
            Formula::True => "true".into(),
            Formula::False => "false".into(),
            Formula::Atom(name) => name.clone(),
            Formula::Not(arg) => wrap(format!("!{}", rec(arg, 6))),
            Formula::Next(arg) => wrap(format!("X {}", rec(arg, 7))),
            Formula::Globally(arg) => wrap(format!("G {}", rec(arg, 7))),
            Formula::Finally(arg) => wrap(format!("F {}", rec(arg, 7))),
            Formula::Yesterday(arg) => wrap(format!("Y {}", rec(arg, 7))),
            Formula::Once(arg) => wrap(format!("O {}", rec(arg, 7))),
            Formula::Historically(arg) => wrap(format!("H {}", rec(arg, 7))),
            Formula::And(l, r) => wrap(format!("{} && {}", rec(l, 4), rec(r, 5))),
            Formula::Or(l, r) => wrap(format!("{} || {}", rec(l, 3), rec(r, 4))),
            Formula::Implies(l, r) => wrap(format!("{} -> {}", rec(l, 3), rec(r, 2))),
            Formula::Iff(l, r) => wrap(format!("{} <-> {}", rec(l, 1), rec(r, 2))),
            Formula::Until(l, r) => wrap(format!("{} U {}", rec(l, 5), rec(r, 6))),
            Formula::Weak(l, r) => wrap(format!("{} W {}", rec(l, 5), rec(r, 6))),
            Formula::Release(l, r) => wrap(format!("{} R {}", rec(l, 5), rec(r, 6))),
            Formula::Since(l, r) => wrap(format!("{} S {}", rec(l, 5), rec(r, 6))),
        }
    }
    rec(f, 0)
}

impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&print_ltl(self))
    }
}

// ---------------------------------------------------------------------------
// JSON serialization — the TS engine's tagged AST shape
// ({kind:"atom",name}, {kind:"not",arg}, {kind:"until",left,right})
// ---------------------------------------------------------------------------

fn kind_name(f: &Formula) -> &'static str {
    match f {
        Formula::True => "true",
        Formula::False => "false",
        Formula::Atom(_) => "atom",
        Formula::Not(..) => "not",
        Formula::And(..) => "and",
        Formula::Or(..) => "or",
        Formula::Implies(..) => "implies",
        Formula::Iff(..) => "iff",
        Formula::Next(..) => "next",
        Formula::Globally(..) => "globally",
        Formula::Finally(..) => "finally",
        Formula::Yesterday(..) => "yesterday",
        Formula::Once(..) => "once",
        Formula::Historically(..) => "historically",
        Formula::Until(..) => "until",
        Formula::Weak(..) => "weak",
        Formula::Release(..) => "release",
        Formula::Since(..) => "since",
    }
}

impl serde::Serialize for Formula {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", kind_name(self))?;
        match self {
            Formula::Atom(name) => map.serialize_entry("name", name)?,
            Formula::Not(arg)
            | Formula::Next(arg)
            | Formula::Globally(arg)
            | Formula::Finally(arg)
            | Formula::Yesterday(arg)
            | Formula::Once(arg)
            | Formula::Historically(arg) => map.serialize_entry("arg", arg)?,
            Formula::And(l, r)
            | Formula::Or(l, r)
            | Formula::Implies(l, r)
            | Formula::Iff(l, r)
            | Formula::Until(l, r)
            | Formula::Weak(l, r)
            | Formula::Release(l, r)
            | Formula::Since(l, r) => {
                map.serialize_entry("left", l)?;
                map.serialize_entry("right", r)?;
            }
            Formula::True | Formula::False => {}
        }
        map.end()
    }
}

/// Deserialization mirror of [`Formula`] — the same tagged AST shape the
/// TypeScript engine uses, so formulas can cross the process boundary.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum FormulaJson {
    True,
    False,
    Atom { name: String },
    Not { arg: Box<FormulaJson> },
    And { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Or { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Implies { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Iff { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Next { arg: Box<FormulaJson> },
    Globally { arg: Box<FormulaJson> },
    Finally { arg: Box<FormulaJson> },
    Yesterday { arg: Box<FormulaJson> },
    Once { arg: Box<FormulaJson> },
    Historically { arg: Box<FormulaJson> },
    Until { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Weak { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Release { left: Box<FormulaJson>, right: Box<FormulaJson> },
    Since { left: Box<FormulaJson>, right: Box<FormulaJson> },
}

impl From<FormulaJson> for Formula {
    fn from(j: FormulaJson) -> Self {
        match j {
            FormulaJson::True => Formula::True,
            FormulaJson::False => Formula::False,
            FormulaJson::Atom { name } => Formula::Atom(name),
            FormulaJson::Not { arg } => Formula::Not(Box::new((*arg).into())),
            FormulaJson::And { left, right } => {
                Formula::And(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Or { left, right } => {
                Formula::Or(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Implies { left, right } => {
                Formula::Implies(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Iff { left, right } => {
                Formula::Iff(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Next { arg } => Formula::Next(Box::new((*arg).into())),
            FormulaJson::Globally { arg } => {
                Formula::Globally(Box::new((*arg).into()))
            }
            FormulaJson::Finally { arg } => Formula::Finally(Box::new((*arg).into())),
            FormulaJson::Yesterday { arg } => {
                Formula::Yesterday(Box::new((*arg).into()))
            }
            FormulaJson::Once { arg } => Formula::Once(Box::new((*arg).into())),
            FormulaJson::Historically { arg } => {
                Formula::Historically(Box::new((*arg).into()))
            }
            FormulaJson::Until { left, right } => {
                Formula::Until(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Weak { left, right } => {
                Formula::Weak(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Release { left, right } => {
                Formula::Release(Box::new((*left).into()), Box::new((*right).into()))
            }
            FormulaJson::Since { left, right } => {
                Formula::Since(Box::new((*left).into()), Box::new((*right).into()))
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for Formula {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(FormulaJson::deserialize(deserializer)?.into())
    }
}

/// Number of nodes in the formula's syntax tree.
pub fn ast_size(f: &Formula) -> usize {
    match f {
        Formula::True | Formula::False | Formula::Atom(_) => 1,
        Formula::Not(arg)
        | Formula::Next(arg)
        | Formula::Globally(arg)
        | Formula::Finally(arg)
        | Formula::Yesterday(arg)
        | Formula::Once(arg)
        | Formula::Historically(arg) => 1 + ast_size(arg),
        Formula::And(l, r)
        | Formula::Or(l, r)
        | Formula::Implies(l, r)
        | Formula::Iff(l, r)
        | Formula::Until(l, r)
        | Formula::Weak(l, r)
        | Formula::Release(l, r)
        | Formula::Since(l, r) => 1 + ast_size(l) + ast_size(r),
    }
}

fn children(f: &Formula) -> Vec<&Formula> {
    match f {
        Formula::True | Formula::False | Formula::Atom(_) => vec![],
        Formula::Not(arg)
        | Formula::Next(arg)
        | Formula::Globally(arg)
        | Formula::Finally(arg)
        | Formula::Yesterday(arg)
        | Formula::Once(arg)
        | Formula::Historically(arg) => vec![arg],
        Formula::And(l, r)
        | Formula::Or(l, r)
        | Formula::Implies(l, r)
        | Formula::Iff(l, r)
        | Formula::Until(l, r)
        | Formula::Weak(l, r)
        | Formula::Release(l, r)
        | Formula::Since(l, r) => vec![l, r],
    }
}

/// All distinct subformulas, post-order, deduplicated by printed form,
/// smallest first.
pub fn subformulas(f: &Formula) -> Vec<Formula> {
    let mut seen: BTreeMap<String, Formula> = BTreeMap::new();
    fn visit(f: &Formula, seen: &mut BTreeMap<String, Formula>) {
        for c in children(f) {
            visit(c, seen);
        }
        seen.entry(print_ltl(f)).or_insert_with(|| f.clone());
    }
    visit(f, &mut seen);
    let mut subs: Vec<Formula> = seen.into_values().collect();
    subs.sort_by(|a, b| ast_size(a).cmp(&ast_size(b)).then_with(|| print_ltl(a).cmp(&print_ltl(b))));
    subs
}

/// The set of atomic proposition names occurring in the formula.
pub fn collect_atoms(f: &Formula) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    fn visit(f: &Formula, atoms: &mut BTreeSet<String>) {
        if let Formula::Atom(name) = f {
            atoms.insert(name.clone());
        }
        for c in children(f) {
            visit(c, atoms);
        }
    }
    visit(f, &mut atoms);
    atoms
}

// ---------------------------------------------------------------------------
// Evaluation — finite traces (LTLf)
// ---------------------------------------------------------------------------

/// Evaluate `f` at position `i` of a finite trace (LTLf semantics).
pub fn eval_at(f: &Formula, trace: &Trace, i: usize) -> bool {
    let n = trace.len();
    match f {
        Formula::True => true,
        Formula::False => false,
        Formula::Atom(name) => i < n && trace[i].contains(name),
        Formula::Not(arg) => !eval_at(arg, trace, i),
        Formula::And(l, r) => eval_at(l, trace, i) && eval_at(r, trace, i),
        Formula::Or(l, r) => eval_at(l, trace, i) || eval_at(r, trace, i),
        Formula::Implies(l, r) => !eval_at(l, trace, i) || eval_at(r, trace, i),
        Formula::Iff(l, r) => eval_at(l, trace, i) == eval_at(r, trace, i),
        Formula::Next(arg) => i + 1 < n && eval_at(arg, trace, i + 1),
        Formula::Globally(arg) => (i..n).all(|j| eval_at(arg, trace, j)),
        Formula::Finally(arg) => (i..n).any(|j| eval_at(arg, trace, j)),
        // Past operators look back over concrete history — the consumed
        // prefix is finite, so they evaluate directly, no window needed.
        // Positions ≥ n are part of "the past" when i is out of range
        // (vacuous quantifiers can hold there), matching the TS engine.
        Formula::Yesterday(arg) => i > 0 && eval_at(arg, trace, i - 1),
        Formula::Once(arg) => (0..=i).any(|j| eval_at(arg, trace, j)),
        Formula::Historically(arg) => (0..=i).all(|j| eval_at(arg, trace, j)),
        Formula::Since(l, r) => {
            // Exists j ≤ i where r held, and l held at every k in (j, i].
            (0..=i).any(|j| {
                eval_at(r, trace, j) && (j + 1..=i).all(|k| eval_at(l, trace, k))
            })
        }
        Formula::Until(l, r) => {
            for j in i..n {
                if eval_at(r, trace, j) {
                    return (i..j).all(|k| eval_at(l, trace, k));
                }
            }
            false
        }
        Formula::Weak(l, r) => {
            eval_at(&Formula::Until(l.clone(), r.clone()), trace, i)
                || eval_at(&Formula::Globally(l.clone()), trace, i)
        }
        Formula::Release(l, r) => !eval_at(
            &Formula::Until(
                Box::new(Formula::Not(l.clone())),
                Box::new(Formula::Not(r.clone())),
            ),
            trace,
            i,
        ),
    }
}

/// Does a finite trace satisfy the formula at position 0 (LTLf)?
pub fn satisfies_trace(f: &Formula, trace: &Trace) -> bool {
    eval_at(f, trace, 0)
}

// ---------------------------------------------------------------------------
// Evaluation — ultimately periodic traces (LTL, for model checking)
// ---------------------------------------------------------------------------

/// The atoms holding at position `i` of an ultimately-periodic run.
///
/// No cycle: a finite prefix is read as stuttering its last state
/// (the dead-end convention), and an empty run as the empty state.
pub fn periodic_atoms(pt: &PeriodicTrace, i: usize) -> &State {
    if i < pt.prefix.len() {
        &pt.prefix[i]
    } else if pt.cycle.is_empty() {
        pt.prefix.last().unwrap_or(&EMPTY_STATE)
    } else {
        &pt.cycle[(i - pt.prefix.len()) % pt.cycle.len()]
    }
}

static EMPTY_STATE: State = State::new();

/// Evaluate at position `i` of an infinite ultimately-periodic run.
/// Positions beyond `prefix.len() + cycle.len()` repeat, so checking one
/// full cycle window past the prefix is sufficient for F/G/U.
pub fn eval_periodic_at(f: &Formula, pt: &PeriodicTrace, i: usize) -> bool {
    if pt.cycle.is_empty() {
        return eval_at(f, &pt.prefix, i);
    }
    // Past the prefix, atoms repeat with period L, so one full period from `i`
    // (or prefix + one period, when `i` is inside the prefix) is decisive.
    let end = i.max(pt.prefix.len()) + pt.cycle.len();
    match f {
        Formula::True => true,
        Formula::False => false,
        Formula::Atom(name) => periodic_atoms(pt, i).contains(name),
        Formula::Not(arg) => !eval_periodic_at(arg, pt, i),
        Formula::And(l, r) => eval_periodic_at(l, pt, i) && eval_periodic_at(r, pt, i),
        Formula::Or(l, r) => eval_periodic_at(l, pt, i) || eval_periodic_at(r, pt, i),
        Formula::Implies(l, r) => !eval_periodic_at(l, pt, i) || eval_periodic_at(r, pt, i),
        Formula::Iff(l, r) => eval_periodic_at(l, pt, i) == eval_periodic_at(r, pt, i),
        Formula::Next(arg) => eval_periodic_at(arg, pt, i + 1),
        Formula::Globally(arg) => (i..end).all(|j| eval_periodic_at(arg, pt, j)),
        Formula::Finally(arg) => (i..end).any(|j| eval_periodic_at(arg, pt, j)),
        // Past operators look back over concrete history (indices 0..=i) —
        // finite even on an infinite run, so no period/window reasoning.
        Formula::Yesterday(arg) => i > 0 && eval_periodic_at(arg, pt, i - 1),
        Formula::Once(arg) => (0..=i).any(|j| eval_periodic_at(arg, pt, j)),
        Formula::Historically(arg) => (0..=i).all(|j| eval_periodic_at(arg, pt, j)),
        Formula::Since(l, r) => (0..=i).any(|j| {
            eval_periodic_at(r, pt, j) && (j + 1..=i).all(|k| eval_periodic_at(l, pt, k))
        }),
        Formula::Until(l, r) => {
            for j in i..end {
                if eval_periodic_at(r, pt, j) {
                    return (i..j).all(|k| eval_periodic_at(l, pt, k));
                }
            }
            false
        }
        Formula::Weak(l, r) => {
            eval_periodic_at(&Formula::Until(l.clone(), r.clone()), pt, i)
                || eval_periodic_at(&Formula::Globally(l.clone()), pt, i)
        }
        Formula::Release(l, r) => !eval_periodic_at(
            &Formula::Until(
                Box::new(Formula::Not(l.clone())),
                Box::new(Formula::Not(r.clone())),
            ),
            pt,
            i,
        ),
    }
}

/// Does an ultimately-periodic run satisfy the formula (infinite LTL semantics)?
pub fn satisfies_periodic(f: &Formula, pt: &PeriodicTrace) -> bool {
    eval_periodic_at(f, pt, 0)
}

// ---------------------------------------------------------------------------
// Truth tables (teaching aid: every subformula, position by position)
// ---------------------------------------------------------------------------

/// One row of a truth table: the atoms at a position plus the truth value of
/// every subformula there.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TruthRow {
    pub atoms: State,
    pub values: Vec<bool>,
}

/// A truth table over a trace: every subformula as a column, every position
/// as a row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TruthTable {
    pub formula: String,
    pub columns: Vec<String>,
    pub rows: Vec<TruthRow>,
    pub result: bool,
    /// Positions where the formula itself reads false — where the run sins.
    pub bad_positions: Vec<usize>,
}

fn build_table(
    f: &Formula,
    rows: Vec<TruthRow>,
    result: bool,
) -> TruthTable {
    let subs = subformulas(f);
    let columns: Vec<String> = subs.iter().map(print_ltl).collect();
    let last = columns.len() - 1;
    let bad_positions = (0..rows.len())
        .filter(|&i| !rows[i].values[last])
        .collect();
    TruthTable {
        formula: print_ltl(f),
        columns,
        rows,
        result,
        bad_positions,
    }
}

/// Truth table of a formula over a finite trace (LTLf).
pub fn truth_table(f: &Formula, trace: &Trace) -> TruthTable {
    let subs = subformulas(f);
    let rows = trace
        .iter()
        .enumerate()
        .map(|(i, atoms)| TruthRow {
            atoms: atoms.clone(),
            values: subs.iter().map(|s| eval_at(s, trace, i)).collect(),
        })
        .collect();
    build_table(f, rows, eval_at(f, trace, 0))
}

/// Truth table over `positions` steps of an ultimately-periodic run
/// (for showing loops).
pub fn periodic_truth_table(f: &Formula, pt: &PeriodicTrace, positions: usize) -> TruthTable {
    let subs = subformulas(f);
    let rows = (0..positions)
        .map(|i| TruthRow {
            atoms: periodic_atoms(pt, i).clone(),
            values: subs.iter().map(|s| eval_periodic_at(s, pt, i)).collect(),
        })
        .collect();
    build_table(f, rows, eval_periodic_at(f, pt, 0))
}
