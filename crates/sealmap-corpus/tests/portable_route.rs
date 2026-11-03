//! A generic consumer needs neither Agentbox's provider names nor its four
//! diagram activities. Unknown identities still cannot earn review credit.
use sealmap_corpus::route::{Family, FamilyCatalogue, Route, RouteError, Rung, cross_family, cross_family_with};
use sealmap_corpus::tiers::Tiers;

#[test]
fn generic_provider_and_model_namespace_are_data_not_an_execution_route() {
    let route = Route::parse("local:team/model:latest -> gateway:google/gemini-pro").unwrap();
    assert_eq!(route.rungs[0].host, "local");
    assert_eq!(route.rungs[0].model, "team/model:latest");
    assert_eq!(
        Route::parse("gateway:google/gemini-pro").unwrap().families().unwrap().into_iter().next().unwrap().to_string(),
        "google"
    );
    cross_family("gateway:anthropic/claude-sonnet", "local:google/gemini-pro").unwrap();
}

#[test]
fn arbitrary_valid_activities_work_without_an_estate_manifest() {
    let tiers =
        Tiers::parse(r#"{"version":1,"corpora":{"docs":{"review-security":"gateway:google/gemini-pro"}}}"#).unwrap();
    assert_eq!(
        tiers.start_route("docs", "review-security", "local:team/model:latest").unwrap(),
        "gateway:google/gemini-pro"
    );
    assert_eq!(tiers.start_route("docs", "summary", "local:team/model:latest").unwrap(), "local:team/model:latest");
}

#[test]
fn provider_labels_never_supply_an_unknown_models_family() {
    for provider in ["gateway", "claude", "codex", "zai", "google"] {
        let model = format!("{provider}:unknown-model");
        assert!(matches!(Route::parse(&model).unwrap().families(), Err(RouteError::UnknownFamily { .. })));
        assert!(cross_family("openai:gpt-6", &model).is_err());
    }
}

#[test]
fn exact_aliases_work_but_alias_names_do_not_create_independent_families() {
    let mut catalogue = FamilyCatalogue::default();
    catalogue.declare("local:writer", Family::Anthropic).unwrap();
    catalogue.declare("gateway:reviewer", Family::named("Claude").unwrap()).unwrap();
    assert!(matches!(
        cross_family_with("local:writer", "gateway:reviewer", &catalogue),
        Err(RouteError::SameFamily { .. })
    ));
    catalogue.declare("gateway:other", Family::Google).unwrap();
    cross_family_with("local:writer", "gateway:other", &catalogue).unwrap();
    // An alias registration never becomes a provider-wide family fallback.
    assert!(cross_family_with("local:unlisted", "gateway:other", &catalogue).is_err());
    assert!(cross_family("local:writer", "gateway:other").is_err());
}

#[test]
fn custom_family_labels_are_canonical_and_cannot_hide_builtin_identity() {
    let mut catalogue = FamilyCatalogue::default();
    catalogue.declare("local:writer", Family::Custom("Vendor-X".into())).unwrap();
    catalogue.declare("gateway:reviewer", Family::named("vendor-x").unwrap()).unwrap();
    assert!(matches!(
        cross_family_with("local:writer", "gateway:reviewer", &catalogue),
        Err(RouteError::SameFamily { .. })
    ));
    catalogue.declare("local:claude-alias", Family::Custom("Anthropic".into())).unwrap();
    assert!(matches!(
        cross_family_with("anthropic:claude-sonnet", "local:claude-alias", &catalogue),
        Err(RouteError::SameFamily { family: Family::Anthropic, .. })
    ));
    assert!(catalogue.declare("local:broken", Family::Custom(" ".into())).is_err());
}

#[test]
fn catalogue_cannot_override_known_models_or_conflicting_aliases() {
    let mut catalogue = FamilyCatalogue::default();
    assert!(catalogue.declare("local:claude-sonnet", Family::Google).is_err());
    assert!(catalogue.declare("claude:writer", Family::Google).is_err());
    catalogue.declare("local:writer", Family::Anthropic).unwrap();
    catalogue.declare("local:writer", Family::Custom("claude".into())).unwrap();
    assert!(catalogue.declare("local:writer", Family::Google).is_err());
    assert_eq!(Route::parse("local:writer").unwrap().families_with(&catalogue).unwrap(), [Family::Anthropic].into());
    assert!(catalogue.declare("local:a -> local:b", Family::Anthropic).is_err());
}

#[test]
fn shared_prefixes_do_not_establish_a_known_model_family() {
    for model in [
        "gpt-j-6b",
        "gpt-neox",
        "gpt4all",
        "gpt-4all",
        "gpt-4ocean",
        "gpt-ossuary",
        "o1bert",
        "o3x",
        "o4all",
        "claudeish",
        "codexchange",
        "glmish",
        "qwenterprise",
        "qwen3ish",
        "geminify",
    ] {
        let route = format!("local:{model}");
        assert!(
            matches!(Route::parse(&route).unwrap().families(), Err(RouteError::UnknownFamily { .. })),
            "{route} must require an explicit family declaration"
        );
        assert!(cross_family("openai:gpt-6", &route).is_err(), "{route}");
        assert!(cross_family(&route, "anthropic:claude-sonnet").is_err(), "{route}");
        let mut catalogue = FamilyCatalogue::default();
        catalogue.declare(&route, Family::named("other-lab").unwrap()).unwrap();
    }
}

#[test]
fn foreign_models_with_a_shared_prefix_use_their_declared_family() {
    let mut catalogue = FamilyCatalogue::default();
    let family = Family::named("eleutherai").unwrap();
    catalogue.declare("local:gpt-j-6b", family.clone()).unwrap();
    catalogue.declare("gateway:eleutherai/gpt-neox:latest", family.clone()).unwrap();
    catalogue.declare("local:pythia-12b", family.clone()).unwrap();
    for author in ["local:gpt-j-6b", "gateway:eleutherai/gpt-neox:latest"] {
        assert!(matches!(
            cross_family_with(author, "local:pythia-12b", &catalogue),
            Err(RouteError::SameFamily { family: shared, .. }) if shared == family
        ));
        cross_family_with("openai:gpt-6", author, &catalogue).unwrap();
    }
}

#[test]
fn bounded_known_model_names_remain_recognised_and_cannot_be_relabelled() {
    for (model, family) in [
        ("gpt-6-astra", Family::OpenAi),
        ("gpt-4o", Family::OpenAi),
        ("gpt-4o-mini", Family::OpenAi),
        ("gpt-4.1", Family::OpenAi),
        ("gpt-oss-20b", Family::OpenAi),
        ("o1", Family::OpenAi),
        ("o3", Family::OpenAi),
        ("o4-mini", Family::OpenAi),
        ("codex-mini-latest", Family::OpenAi),
        ("claude-sonnet-5-5", Family::Anthropic),
        ("claude-opus", Family::Anthropic),
        ("glm-5.3", Family::Zhipu),
        ("qwen3.8-27B", Family::Alibaba),
        ("qwen2.5-coder", Family::Alibaba),
        ("qwen-3-8b", Family::Alibaba),
        ("gemini-pro", Family::Google),
        ("gemini-3.8-flash", Family::Google),
    ] {
        for spelling in [model.into(), format!("namespace/{}:latest", model.to_uppercase()), format!("{model}@pin")] {
            let route = format!("gateway:{spelling}");
            assert_eq!(Route::parse(&route).unwrap().families().unwrap(), [family.clone()].into(), "{route}");
            let mut catalogue = FamilyCatalogue::default();
            assert!(catalogue.declare(&route, Family::named("independent").unwrap()).is_err(), "{route}");
            catalogue.declare(&route, family.clone()).unwrap();
        }
    }
}

#[test]
fn family_checks_reject_multiple_reviewers_and_manually_malformed_routes() {
    assert!(matches!(
        cross_family("local:claude-sonnet", "google:gemini-pro -> zai:glm-5.3"),
        Err(RouteError::ReviewerRoute(_))
    ));
    assert!(Route { rungs: vec![] }.families().is_err());
    assert!(Rung { host: "bad provider".into(), model: "gpt-6".into() }.family().is_err());
    assert!(Route::parse("openai:claude-sonnet").unwrap().families().is_err());
    for malformed in
        ["", "provider", ":model", "provider:", "provider:model name", "local:a -> local:b -> local:c", "local:a;cat"]
    {
        assert!(Route::parse(malformed).is_err(), "{malformed}");
    }
}

#[test]
fn caller_activity_policy_is_retained_for_defaults_and_missing_files() {
    const FILE: &str = r#"{"version":1,"corpora":{"docs":{"review-security":"google:gemini-pro"}}}"#;
    let tiers = Tiers::parse_with_activities(FILE, &["review-security", "summary"]).unwrap();
    assert!(tiers.start_route("docs", "review-securty", "google:gemini-pro").is_err());
    assert!(Tiers::parse_with_activities(FILE, &["summary"]).is_err());
    let none = Tiers::from_file_text_with_activities(None, &["summary"]).unwrap();
    assert_eq!(none.start_route("docs", "summary", "local:writer").unwrap(), "local:writer");
    assert!(none.start_route("docs", "unknown", "local:writer").is_err());
    assert!(Tiers::from_file_text_with_activities(None, &["bad name"]).is_err());
    assert!(
        Tiers::from_file_text_with_activities(None, &[])
            .unwrap()
            .start_route("docs", "summary", "local:writer")
            .is_err()
    );
}

#[test]
fn agentbox_policy_remains_explicit_and_strict() {
    assert!(Route::parse_agentbox("google:gemini-pro").is_err());
    assert!(Route::parse_agentbox("loom:team/qwen-model:latest").is_err());
    assert!(Route::parse_agentbox("loom:qwen3.8-27B -> claude:claude-sonnet").is_ok());
    assert!(Tiers::parse_agentbox(r#"{"version":1,"corpora":{"docs":{"seal-review":"google:gemini-pro"}}}"#).is_err());
    let none = Tiers::from_file_text_agentbox(None).unwrap();
    assert!(none.start_route("docs", "seal-review", "google:gemini-pro").is_err());
    assert!(none.start_route("docs", "custom-activity", "zai:glm-5.3").is_err());
    assert_eq!(none.start_route("docs", "seal-review", "zai:glm-5.3").unwrap(), "zai:glm-5.3");
}

#[test]
fn portable_records_remain_strict_about_schema_duplicates_and_identifiers() {
    for invalid in [
        r#"{"version":1,"corpora":{"docs":{"review security":"local:writer"}}}"#,
        r#"{"version":1,"corpora":{"docs":{"":"local:writer"}}}"#,
        r#"{"version":1,"corpora":{"":{"review":"local:writer"}}}"#,
        r#"{"version":1,"corpora":{"docs":{"review":"local:writer","review":"local:other"}}}"#,
        r#"{"version":1,"version":1,"corpora":{}}"#,
        r#"{"version":1,"corpora":{"docs":{},"docs":{}}}"#,
        r#"{"version":1,"corpora":{},"unexpected":true}"#,
        r#"{"version":2,"corpora":{}}"#,
    ] {
        assert!(Tiers::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn portable_known_model_identities_reach_the_existing_seal_api() {
    use sealmap_corpus::seal::{Lock, Signature, sign};
    use sealmap_model::{SourcePath, SourceSet};
    let mut sources = SourceSet::new();
    sources.insert("src/lib.rs", "pub fn run() {}\n").unwrap();
    let model =
        sealmap_rust::extract(&sources, &sealmap_rust::RustOptions { name: "example".into(), ..Default::default() })
            .codebase;
    let file = SourcePath::new("example.md").unwrap();
    let topic = "---\nid: EX-01\n---\nCalls `sym:cargo example . run().`.\n";
    let mut lock = Lock::default();
    let mut who = Signature {
        reviewer: "google:gemini-pro".into(),
        model: "gateway:anthropic/claude-sonnet".into(),
        date: "2026-11-03".into(),
    };
    assert!(sign(&mut lock, &file, topic, &model, "seals.lock", &who).is_ok());
    let before = lock.to_toml();
    who.reviewer = "local:anthropic/claude-opus:latest".into();
    assert!(sign(&mut lock, &file, topic, &model, "seals.lock", &who).is_err());
    assert_eq!(lock.to_toml(), before);
}
