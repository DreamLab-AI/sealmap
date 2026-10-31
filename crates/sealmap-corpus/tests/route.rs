//! ADR-2130: a seal review is a different model family from the author, and
//! `.tiers.json` records confirmed de-escalations. Model ids are the ones in
//! agentbox's `[model_routing.routes]`.

use sealmap_corpus::route::{Family, Route, RouteError, cross_family};
use sealmap_corpus::tiers::{TierError, Tiers};

const SEAL_REVIEW: &str = "zai:glm-5.3 -> claude:claude-fable-5-1";

fn fam(route: &str) -> Vec<Family> {
    Route::parse(route).unwrap().families().unwrap().into_iter().collect()
}

#[test]
fn hosts_and_ids_resolve_to_a_family() {
    assert_eq!(fam("claude:claude-sonnet-5-5"), [Family::Anthropic]);
    assert_eq!(fam("codex:gpt-6-astra"), [Family::OpenAi]);
    assert_eq!(fam("zai:glm-5.3"), [Family::Zhipu]);
}

#[test]
fn a_loom_route_resolves_to_the_model_behind_it() {
    assert_eq!(fam("loom:qwen3.8-27B"), [Family::Alibaba]);
    // A swapped-in model is judged by its own id, not by the host.
    assert_eq!(fam("loom:claude-sonnet-5-5"), [Family::Anthropic]);
    assert_eq!(fam("loom:glm-5.3"), [Family::Zhipu]);
}

#[test]
fn an_unrecognised_loom_model_has_an_unknown_family() {
    let err = Route::parse("loom:hemmingway-1.5-q8_0").unwrap().families().unwrap_err();
    assert!(matches!(err, RouteError::UnknownFamily { .. }), "{err}");
}

#[test]
fn a_host_that_contradicts_its_model_id_is_refused() {
    let err = Route::parse("claude:glm-5.3").unwrap().families().unwrap_err();
    assert!(matches!(err, RouteError::HostModelMismatch { .. }), "{err}");
}

#[test]
fn an_escalating_route_carries_the_family_of_every_rung() {
    assert_eq!(fam(SEAL_REVIEW), [Family::Anthropic, Family::Zhipu]);
}

#[test]
fn grammar_is_the_manifest_grammar() {
    for bad in ["", "claude", "gemini:gemini-3.8-flash", "claude:a -> claude:b -> claude:c", "claude:a ->", "claude:"] {
        assert!(Route::parse(bad).is_err(), "{bad:?} must not parse");
    }
    assert!(Route::parse("  claude:x  ->  zai:y ").is_ok());
}

#[test]
fn same_family_is_refused() {
    let err = cross_family("claude:claude-sonnet-5-5", "claude:claude-fable-5-1").unwrap_err();
    assert!(matches!(err, RouteError::SameFamily { family: Family::Anthropic, .. }), "{err}");
    // A reviewer whose fallback rung shares the author's family is refused too.
    assert!(cross_family("claude:claude-sonnet-5-5", SEAL_REVIEW).is_err());
    // Loom resolves through to the family, so a Loom Claude is still Claude.
    assert!(cross_family("loom:claude-sonnet-5-5", "claude:claude-opus-5-5").is_err());
}

#[test]
fn different_families_are_allowed() {
    cross_family("claude:claude-sonnet-5-5", "zai:glm-5.3").unwrap();
    cross_family("codex:gpt-6-astra", "claude:claude-fable-5-1").unwrap();
    cross_family("loom:qwen3.8-27B", "claude:claude-sonnet-5-5").unwrap();
}

#[test]
fn an_unknown_family_is_refused_on_either_side() {
    assert!(cross_family("loom:hemmingway-1.5-q8_0", "claude:claude-sonnet-5-5").is_err());
    assert!(cross_family("claude:claude-sonnet-5-5", "loom:hemmingway-1.5-q8_0").is_err());
    assert!(cross_family("m", "r").is_err());
    assert!(cross_family("", "zai:glm-5.3").is_err());
}

// ------------------------------------------------------------------ .tiers.json

const DEFAULT: &str = "loom:qwen3.8-27B -> claude:claude-sonnet-5-5";

#[test]
fn a_recorded_corpus_starts_from_its_confirmed_route() {
    let t = Tiers::parse(r#"{"version":1,"corpora":{"sealmap":{"route":"claude:claude-sonnet-5-5"}}}"#).unwrap();
    assert_eq!(t.start_route("sealmap", DEFAULT).unwrap(), "claude:claude-sonnet-5-5");
}

#[test]
fn an_unrecorded_corpus_or_an_absent_file_falls_back_to_the_default() {
    let t = Tiers::parse(r#"{"version":1,"corpora":{}}"#).unwrap();
    assert_eq!(t.start_route("sealmap", DEFAULT).unwrap(), DEFAULT);
    assert_eq!(Tiers::from_file_text(None).unwrap().start_route("sealmap", DEFAULT).unwrap(), DEFAULT);
}

#[test]
fn a_malformed_file_is_an_error_never_a_silent_default() {
    for bad in [
        "",
        "not json",
        "[]",
        r#"{"version":2,"corpora":{}}"#,
        r#"{"corpora":{}}"#,
        r#"{"version":1,"corpora":{"a":{}}}"#,
        r#"{"version":1,"corpora":{"a":{"route":"gemini:x"}}}"#,
        r#"{"version":1,"corpora":{"a":{"route":"claude:x","extra":1}}}"#,
    ] {
        let err = Tiers::from_file_text(Some(bad)).unwrap_err();
        assert!(matches!(err, TierError::Malformed(_)), "{bad:?}: {err}");
    }
}

#[test]
fn a_bad_default_route_is_an_error() {
    let t = Tiers::from_file_text(None).unwrap();
    assert!(t.start_route("sealmap", "nonsense").is_err());
}
