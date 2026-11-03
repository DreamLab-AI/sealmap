//! Conservative selection is deliberately broader than symbol diagnostics.
use sealmap_corpus::{pack::changed_topics_by_files, seal::Topics};
use sealmap_model::{SourcePath, SourceSet};
use sealmap_rust::{RustOptions, extract};
use std::collections::BTreeSet;

#[test]
fn file_changes_keep_uncited_context_without_selecting_unrelated_known_topics() {
    let mut sources = SourceSet::new();
    sources.insert("src/lib.rs", "pub fn go() {}\npub fn context() {}\n").unwrap();
    let model = extract(&sources, &RustOptions { name: "app".into(), ..Default::default() }).codebase;
    let mut topics = Topics::new();
    let available = BTreeSet::from([SourcePath::new("src/lib.rs").unwrap(), SourcePath::new("web/client.ts").unwrap()]);
    topics.insert(SourcePath::new("a.md").unwrap(), "---\nid: A\n---\n`sym:cargo app . go().`".into());
    topics.insert(SourcePath::new("b.md").unwrap(), "---\nid: B\nsources: [web/client.ts]\n---\nClient".into());
    topics
        .insert(SourcePath::new("c.md").unwrap(), "---\nid: C\nsources:\n  - '../missing/api.rs'\n---\nUnknown".into());
    let changed = BTreeSet::from([SourcePath::new("src/lib.rs").unwrap()]);
    assert_eq!(changed_topics_by_files(&topics, &changed, &available, &model, &model), ["A", "C"]);
    let changed = BTreeSet::from([SourcePath::new("web/client.ts").unwrap()]);
    assert_eq!(changed_topics_by_files(&topics, &changed, &available, &model, &model), ["B", "C"]);
    let changed = BTreeSet::from([SourcePath::new("Cargo.lock").unwrap()]);
    assert_eq!(changed_topics_by_files(&topics, &changed, &available, &model, &model), ["A", "B", "C"]);
    assert_eq!(changed_topics_by_files(&topics, &BTreeSet::new(), &available, &model, &model), ["C"]);
}

#[test]
fn malformed_or_unsupported_sources_are_never_a_skip_gate() {
    let model = extract(&SourceSet::new(), &RustOptions::default()).codebase;
    let changed = BTreeSet::from([SourcePath::new("other.rs").unwrap()]);
    for front in [
        "sources: [unclosed",
        "sources: &alias",
        "sources: [src/*.rs]",
        "sources:\n  - src/lib.rs\nsources: []",
        "sources: []",
    ] {
        let mut topics = Topics::new();
        topics.insert(SourcePath::new("a.md").unwrap(), format!("---\nid: A\n{front}\n---\nBody"));
        assert_eq!(changed_topics_by_files(&topics, &changed, &BTreeSet::new(), &model, &model), ["A"], "{front}");
    }
}

#[test]
fn missing_and_gitlink_sources_cannot_disappear_from_the_queue() {
    let model = extract(&SourceSet::new(), &RustOptions::default()).codebase;
    let mut topics = Topics::new();
    topics.insert(SourcePath::new("a.md").unwrap(), "---\nid: A\nsources: [src/missing.rs]\n---\nMissing".into());
    topics.insert(
        SourcePath::new("b.md").unwrap(),
        "---\nid: B\nsources: [dependency/src/lib.rs]\n---\nNested repo".into(),
    );
    let available = BTreeSet::from([SourcePath::new("dependency/src/lib.rs").unwrap()]);
    let gitlink = BTreeSet::from([SourcePath::new("dependency").unwrap()]);
    assert_eq!(changed_topics_by_files(&topics, &gitlink, &available, &model, &model), ["A", "B"]);
    assert_eq!(changed_topics_by_files(&topics, &BTreeSet::new(), &available, &model, &model), ["A"]);
}
