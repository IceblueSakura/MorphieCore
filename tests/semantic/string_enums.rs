//! Canonical wire spelling is independent of Rust names and stays fail-closed.
use morphiecore::semantic::task::generation::{
    ImageDetail, Phase, ReasoningContext, ReasoningEffort, ReasoningMode,
};
#[test]
fn closed_labels_parse_and_render_canonical_wire_values() {
    for (wire, value) in [
        ("auto", ImageDetail::Auto),
        ("low", ImageDetail::Low),
        ("high", ImageDetail::High),
        ("original", ImageDetail::Original),
    ] {
        assert_eq!(wire.parse::<ImageDetail>().unwrap(), value);
        assert_eq!(value.label(), wire);
    }
    for (wire, value) in [
        ("commentary", Phase::Commentary),
        ("final_answer", Phase::FinalAnswer),
    ] {
        assert_eq!(wire.parse::<Phase>().unwrap(), value);
        assert_eq!(value.label(), wire);
    }
    for (wire, value) in [
        ("none", ReasoningEffort::None),
        ("minimal", ReasoningEffort::Minimal),
        ("low", ReasoningEffort::Low),
        ("medium", ReasoningEffort::Medium),
        ("high", ReasoningEffort::High),
        ("xhigh", ReasoningEffort::XHigh),
        ("max", ReasoningEffort::Max),
    ] {
        assert_eq!(wire.parse::<ReasoningEffort>().unwrap(), value);
        let label: &'static str = value.into();
        assert_eq!(label, wire);
    }
    for (wire, value) in [
        ("auto", ReasoningContext::Auto),
        ("current_turn", ReasoningContext::CurrentTurn),
        ("all_turns", ReasoningContext::AllTurns),
    ] {
        assert_eq!(wire.parse::<ReasoningContext>().unwrap(), value);
        let label: &'static str = value.into();
        assert_eq!(label, wire);
    }
    for (wire, value) in [
        ("standard", ReasoningMode::Standard),
        ("pro", ReasoningMode::Pro),
    ] {
        assert_eq!(wire.parse::<ReasoningMode>().unwrap(), value);
        let label: &'static str = value.into();
        assert_eq!(label, wire);
    }
}
#[test]
fn parsing_never_accepts_rust_names_case_folding_or_whitespace() {
    for label in ["Auto", "LOW", " low", "low ", "", "unknown"] {
        assert!(label.parse::<ImageDetail>().is_err());
        assert!(label.parse::<ReasoningEffort>().is_err());
    }
    for label in ["FinalAnswer", "finalanswer", "final_answer "] {
        assert!(label.parse::<Phase>().is_err());
    }
    for label in ["CurrentTurn", "currentturn", "all_turns "] {
        assert!(label.parse::<ReasoningContext>().is_err());
    }
    for label in ["Standard", "PRO", " pro"] {
        assert!(label.parse::<ReasoningMode>().is_err());
    }
}
