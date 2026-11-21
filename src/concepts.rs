//! The field guide: what each formal-methods concept *is*, why it matters for
//! testing, and how it shows up in this crate.  Agent-facing tooling (an MCP
//! `explain_concept`, a CLI) can serve exactly this content so the prose and
//! the engine never disagree.
//!
//! House style: one idea per paragraph, three sentences at most.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// A section of a concept chapter: a heading, prose paragraphs, and an
/// optional bullet list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConceptSection {
    pub heading: &'static str,
    pub paragraphs: &'static [&'static str],
    pub bullets: &'static [&'static str],
}

/// One chapter of the field guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Concept {
    pub id: &'static str,
    pub title: &'static str,
    pub tagline: &'static str,
    pub sections: &'static [ConceptSection],
    pub related: &'static [&'static str],
}

/// All chapters, in guide order.
pub static CONCEPTS: &[Concept] = &[
    Concept {
        id: "specification",
        title: "Specification as the test oracle",
        tagline: "Before you can test a system, decide what counts as a failure.",
        sections: &[
            ConceptSection {
                heading: "The oracle problem",
                paragraphs: &[
                    "A test needs two things: an input, and a way to tell pass from fail. The oracle is the expensive half — expected behaviour lives in tickets and heads, where it rots.",
                    "A formal spec replaces all of that with one precise sentence: `G (request -> F response)` — *always*, a request is *eventually* answered. One line, an oracle for infinitely many tests.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "From property to verdict",
                paragraphs: &[
                    "Once the property is machine-checkable, the loop inverts: *generate* executions, ask the spec whether each one passes. This crate does that three ways — exhaustively (model checking), randomly (property-based testing), and after the fact (runtime verification on logs).",
                ],
                bullets: &[
                    "`G p` — safety: `p` holds in every reachable state.",
                    "`F p` — liveness: `p` eventually happens.",
                    "`p U q` — precedence: `p` holds until `q`, which really occurs.",
                    "`G (request -> F response)` — the request/response contract.",
                ],
            },
        ],
        related: &["ltl", "model-checking", "property-based-testing"],
    },
    Concept {
        id: "kripke",
        title: "Kripke structures",
        tagline: "The machine you *intend* to build, written so a machine can read it.",
        sections: &[
            ConceptSection {
                heading: "States, labels, transitions",
                paragraphs: &[
                    "A Kripke structure is a directed graph: **states**, each labelled with the **atomic propositions** true there; **transitions** say which state may follow which; **initial states** say where runs begin.",
                    "That is deliberately less than real code says — no variables, no clocks. Modelling *is* abstraction: keep exactly enough to state what correct means.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "Nondeterminism is a feature",
                paragraphs: &[
                    "A state with two outgoing transitions does not choose — the environment does. Checking the structure checks *every* resolution, so an adversary is covered before it exists.",
                    "Dead ends **stutter forever** (the system has frozen). That one convention is what lets liveness properties catch hangs.",
                ],
                bullets: &[],
            },
        ],
        related: &["model-checking", "bounded-model-checking"],
    },
    Concept {
        id: "ltl",
        title: "Linear temporal logic",
        tagline: "A tiny language for talking about time — the backbone of this crate.",
        sections: &[
            ConceptSection {
                heading: "The grammar",
                paragraphs: &[
                    "LTL adds temporal operators to ordinary propositional logic. Formulas are evaluated **along a run**, one state at a time, from the first state onward.",
                ],
                bullets: &[
                    "`X p` — *next*: `p` holds in the state that follows.",
                    "`F p` — *finally*: `p` holds somewhere later (`<>`).",
                    "`G p` — *globally*: `p` holds at every step (`[]`).",
                    "`p U q` — *until*: `p` holds until `q` occurs, which it eventually does.",
                    "`p W q` — weak until: drops the eventual obligation; `q R p` is its dual.",
                ],
            },
            ConceptSection {
                heading: "Two semantics, one grammar",
                paragraphs: &[
                    "Over an **infinite** run — the textbook reading — `G` and `F` quantify over an endless future. The model checker uses this one.",
                    "Over a **finite** trace — a test case, a log — `F p` only promises `p` *within what you saw*, and `X p` is false at the last step. That reading is LTLf, the right one for logs and concrete tests.",
                    "This crate is bilingual: the model checker sees infinite runs; `satisfies_trace` and the fuzzer see finite traces. Know which language you are speaking before you compare answers.",
                ],
                bullets: &[],
            },
        ],
        related: &["specification", "runtime-verification", "model-checking"],
    },
    Concept {
        id: "model-checking",
        title: "Model checking",
        tagline: "Don't pick test cases. Check all of them.",
        sections: &[
            ConceptSection {
                heading: "Every behaviour at once",
                paragraphs: &[
                    "Model checking asks: *does every possible run of the structure satisfy the property?* Not a sample — all of them, including the interleaving you never thought to test.",
                    "The answer \"no\" arrives with a **counterexample**: a concrete run that breaks the property. That counterexample *is* a failing test, ready for the regression suite.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "What it costs",
                paragraphs: &[
                    "The price is the **state explosion**: runs grow exponentially with states and nondeterminism, and full checking is PSPACE-complete. In practice you check small abstractions — this crate enumerates **lassos** (simple prefix + loop) explicitly, right for teaching and models of a few dozen states.",
                    "Too big? Bound the search — that is bounded model checking, and where testing takes over.",
                ],
                bullets: &[],
            },
        ],
        related: &["bounded-model-checking", "kripke", "coverage"],
    },
    Concept {
        id: "bounded-model-checking",
        title: "Bounded model checking",
        tagline: "Exhaustive within a budget — the honest middle between proving and testing.",
        sections: &[
            ConceptSection {
                heading: "Search, but only so deep",
                paragraphs: &[
                    "BMC asks a weaker question on purpose: *does any run of length ≤ k violate the property?* Translate bounded runs into a SAT/SMT query and let a solver hunt for the bad ones only — that scales far past explicit enumeration.",
                    "The verdict means exactly what testers already live with: **no counterexample within depth k** — what a campaign covering the first `k` steps can claim, and no more.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "Sound, never complete",
                paragraphs: &[
                    "Raising `k` alone can never yield a proof — a deeper counterexample may hide below the bound. So the answer is always: violation found, or *unknown*. Honest tools say unknown out loud.",
                    "Special case: a finite structure means every violation is a **lasso**. At depth = number of states, this crate's `holds` is genuinely exhaustive; below that, it labels itself `incomplete`.",
                ],
                bullets: &[],
            },
        ],
        related: &["model-checking", "test-generation"],
    },
    Concept {
        id: "property-based-testing",
        title: "Property-based testing & falsification",
        tagline: "Let the machine try to break your spec — then let it explain itself.",
        sections: &[
            ConceptSection {
                heading: "Random executions, one property",
                paragraphs: &[
                    "Property-based testing replaces hand-written cases with a *generator* plus a *predicate*: invent inputs at random, keep the property that must hold for all of them. A few thousand runs covers cases nobody would type — the empty trace, everything at once, the pathological order.",
                    "Here the artefacts are **traces of propositions**, judged by an LTL formula. The fuzzer invents runs; the formula votes.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "Shrinking: counterexamples that explain",
                paragraphs: &[
                    "A raw failure is noise — 12 random steps when 2 would do. **Shrinking** keeps deleting a step or an atom while the trace *still* violates; what survives is a minimal, readable reproduction.",
                    "A passing campaign proves nothing — you sampled and stayed lucky. But a shrunk failure teaches you something no example-based test would.",
                ],
                bullets: &[
                    "Seeds make runs reproducible — same seed, same failures.",
                    "Coverage-guided fuzzers bend generation toward unexplored paths.",
                    "Surviving 10⁵ random traces is *evidence*, not proof.",
                ],
            },
        ],
        related: &["runtime-verification", "specification", "coverage"],
    },
    Concept {
        id: "coverage",
        title: "Coverage as a negative metric",
        tagline: "Coverage tells you what you didn't test. Never what you did.",
        sections: &[
            ConceptSection {
                heading: "Measured honesty",
                paragraphs: &[
                    "Coverage answers one question with perfect precision: *which states and transitions did my tests never reach?* Run your traces; the report lists every miss. Uncovered means no test exercised it — a fact.",
                    "The trap is reading it backwards: 100% coverage means every edge fired once, not that every behaviour was tested. Coverage is a checklist of remaining work, never a certificate of correctness.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "Spec coverage",
                paragraphs: &[
                    "The second axis: *requirement coverage* — does every clause of the spec have a test that would fail if the clause were broken? If the model can violate the property, make sure a test drives it there — or let the model checker: its counterexamples are pre-loaded failing tests.",
                ],
                bullets: &[],
            },
        ],
        related: &["test-generation", "model-checking"],
    },
    Concept {
        id: "runtime-verification",
        title: "Runtime verification",
        tagline: "Check the property against what actually ran — in CI, in production.",
        sections: &[
            ConceptSection {
                heading: "From logs to verdicts",
                paragraphs: &[
                    "Runtime verification compiles the formula into a **monitor**: a small state machine that consumes events — a test, a request log, a CI run — and flags a violation the moment it becomes inevitable. It speaks the finite dialect: `F response` promises *within the log*.",
                    "Model checking reasons about all possible runs; this reasons about the one that happened. Cheap, no model needed, deployable today.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "A CI-native oracle",
                paragraphs: &[
                    "Write the property once, reuse it three ways: as a monitor in CI, as the fuzzer's predicate, as the model checker's target. One spec, three execution modes, no drift.",
                ],
                bullets: &[],
            },
        ],
        related: &["ltl", "property-based-testing"],
    },
    Concept {
        id: "test-generation",
        title: "Specification-based test generation",
        tagline: "The spec already knows which tests you need. Let it write them.",
        sections: &[
            ConceptSection {
                heading: "Tests from a model",
                paragraphs: &[
                    "A test case is a finite path through the model — a story the system can live. Generate runs, keep the interesting ones, export them as replayable cases.",
                    "Model checking contributes the best cases for free (counterexamples provably fail). Coverage lists the rest — the runs you still owe a test.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "Why this beats example-based testing",
                paragraphs: &[
                    "Hand-written tests check yesterday's bugs; generated tests check standing obligations. The catch is bookkeeping: tests are only as good as the model — keep it small, reviewed, and in the repo.",
                ],
                bullets: &[],
            },
        ],
        related: &["model-checking", "coverage"],
    },
    Concept {
        id: "beyond",
        title: "Beyond checking: proving",
        tagline: "SAT, SMT, and proof assistants — when exhaustive still isn't enough.",
        sections: &[
            ConceptSection {
                heading: "The wider toolbox",
                paragraphs: &[
                    "Model checking finite structures is the friendly corner of formal methods. Unbounded state extends the toolbox: **SAT/SMT** for bounded checking over infinite data, **theorem provers** (Coq, Lean, Isabelle) that prove all instances at once, **abstraction** that over-approximates infinite systems.",
                    "For testing teams the bridge stays the same: keep the model, let a solver search it.",
                ],
                bullets: &[],
            },
            ConceptSection {
                heading: "The honest spectrum",
                paragraphs: &[
                    "Confidence vs cost: example tests (cheap, weak) → random falsification → bounded checking → full model checking (exhaustive, needs abstraction) → proof assistants (total, expensive). The craft is picking the strongest tool you can still afford — and labelling the answer with what it proves.",
                ],
                bullets: &[],
            },
        ],
        related: &["bounded-model-checking", "model-checking"],
    },
];

/// Look up a chapter by id.
pub fn get_concept(id: &str) -> Option<&'static Concept> {
    CONCEPTS.iter().find(|c| c.id == id)
}

/// A flat `(id, title, tagline)` index of all chapters.
pub fn concept_index() -> Vec<(&'static str, &'static str, &'static str)> {
    CONCEPTS
        .iter()
        .map(|c| (c.id, c.title, c.tagline))
        .collect()
}
