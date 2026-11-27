use formalmethods::{CONCEPTS, SKILLS_GUIDE, concept_index, get_concept};

#[test]
fn serves_the_eleven_chapters_in_order() {
    let ids: Vec<&str> = CONCEPTS.iter().map(|c| c.id).collect();
    assert_eq!(
        ids,
        [
            "specification",
            "kripke",
            "ltl",
            "model-checking",
            "ctl-vs-ltl",
            "bounded-model-checking",
            "property-based-testing",
            "coverage",
            "runtime-verification",
            "test-generation",
            "beyond",
        ]
    );
}

#[test]
fn index_and_lookup_agree() {
    for (id, title, _) in concept_index() {
        assert_eq!(get_concept(id).unwrap().title, title);
    }
    assert!(get_concept("nonsense").is_none());
}

#[test]
fn ctl_chapter_teaches_the_composition_story() {
    let c = get_concept("ctl-vs-ltl").unwrap();
    let text: String = c
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter().copied())
        .collect();
    assert!(text.contains("ctl_check"));
    assert!(text.contains("model_check"));
}

#[test]
fn skills_guide_is_the_operating_manual() {
    // served verbatim by `fm read_skills_guide` — must mention every tool
    for tool in [
        "check_ltl",
        "monitor_ltl",
        "model_check",
        "ctl_check",
        "analyze_depth",
        "falsify_property",
        "analyze_coverage",
        "analyze_spec_coverage",
        "mutation_report",
        "explain_concept",
        "read_skills_guide",
    ] {
        assert!(SKILLS_GUIDE.contains(tool), "guide should mention `{tool}`");
    }
    for preset in ["vending", "vending-eaten", "traffic-light"] {
        assert!(SKILLS_GUIDE.contains(preset), "guide should mention preset `{preset}`");
    }
}
