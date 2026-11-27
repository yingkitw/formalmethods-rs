//! Named Kripke presets — the same machines the TS workbench ships,
//! accepted by the `fm` model-taking tools as a `preset` argument
//! (`"vending"`, `"vending-eaten"`, `"traffic-light"`).

use crate::model::KripkeSpec;

/// Vending machine, correct: paid always vends.
pub fn vending() -> KripkeSpec {
    KripkeSpec {
        atoms: ["ready", "coin", "dispense"].iter().map(|s| s.to_string()).collect(),
        initial: vec!["idle".into()],
        states: vec![
            crate::model::KripkeState {
                id: "idle".into(),
                atoms: ["ready"].iter().map(|s| s.to_string()).collect(),
                next: vec!["paid".into()],
            },
            crate::model::KripkeState {
                id: "paid".into(),
                atoms: ["coin"].iter().map(|s| s.to_string()).collect(),
                next: vec!["vend".into()],
            },
            crate::model::KripkeState {
                id: "vend".into(),
                atoms: ["dispense"].iter().map(|s| s.to_string()).collect(),
                next: vec!["idle".into()],
            },
        ],
    }
}

/// Vending machine with the nondeterministic coin-eating bug: `paid` may
/// return to `idle` instead of vending.
pub fn vending_eaten() -> KripkeSpec {
    let mut m = vending();
    m.states[1].next = vec!["idle".into(), "vend".into()];
    m
}

/// Traffic light that can stay red forever — a liveness failure.
pub fn stuck_light() -> KripkeSpec {
    KripkeSpec {
        atoms: ["stop", "go"].iter().map(|s| s.to_string()).collect(),
        initial: vec!["red".into()],
        states: vec![
            crate::model::KripkeState {
                id: "red".into(),
                atoms: ["stop"].iter().map(|s| s.to_string()).collect(),
                next: vec!["red".into(), "green".into()],
            },
            crate::model::KripkeState {
                id: "green".into(),
                atoms: ["go"].iter().map(|s| s.to_string()).collect(),
                next: vec!["amber".into()],
            },
            crate::model::KripkeState {
                id: "amber".into(),
                atoms: Default::default(),
                next: vec!["red".into()],
            },
        ],
    }
}

/// Resolve a preset by its `fm`/MCP name.
pub fn preset(name: &str) -> Option<KripkeSpec> {
    match name {
        "vending" => Some(vending()),
        "vending-eaten" => Some(vending_eaten()),
        "traffic-light" => Some(stuck_light()),
        _ => None,
    }
}

/// Every preset name, for error messages and docs.
pub fn preset_names() -> &'static [&'static str] {
    &["vending", "vending-eaten", "traffic-light"]
}
