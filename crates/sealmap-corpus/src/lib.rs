//! # sealmap-corpus
//!
//! Three jobs: the generated 1:1 Mermaid corpus (below); [`seal`], the
//! lockfile and checks that keep hand-written diagram topics true to the
//! code they cite (`sealmap verify`); and [`pack`], bounded review packs of
//! topics with the code they cite (`sealmap pack`).
//!
//! Projects a [`Codebase`] into a **contract-enforced, 1:1 corpus** of dense
//! Mermaid diagrams plus machine-readable metadata, designed to be read by
//! LLM agents and merged by an orchestrating agent.
//!
//! ## The 1:1 contract
//!
//! For every source file `S` in the codebase the corpus contains exactly one
//! document `S.md`, and nothing else in the corpus claims to describe `S`.
//! Each document records the BLAKE3 hash of the source it was generated from,
//! so drift is detectable without re-parsing. [`verify`] checks the contract
//! against a directory and reports, per file:
//!
//! | Status | Meaning |
//! |---|---|
//! | [`Drift::Missing`] | source exists, document does not |
//! | [`Drift::Orphaned`] | document exists, source does not |
//! | [`Drift::Stale`] | source changed since the document was generated |
//! | [`Drift::Modified`] | source unchanged but document differs (hand edit, or generator/options changed) |
//!
//! [`write()`] brings a directory into compliance (and only ever deletes files
//! that carry the sealmap header); `sealmap generate --check` runs [`verify`]
//! and writes nothing. The corpus is a regenerable view, not a gate.
//!
//! ## What a document contains
//!
//! `src/net/client.rs.md`:
//!
//! 1. YAML front matter: source path, module id, source hash, counts, and the
//!    list of fragment ids in the document.
//! 2. **Structure**: one `classDiagram` of everything defined in the file
//!    (types with fields/variants/methods, a `<<module>>` box for free
//!    functions, constants and submodules) and their relations
//!    (implements, extends, owns, uses), with stubs for types defined
//!    elsewhere.
//! 3. **Sequences**: one `sequenceDiagram` per function or method that makes
//!    at least [`CorpusOptions::min_calls`] calls, rendering its ordered call
//!    flow with `alt`/`opt`/`loop`/`par` fragments.
//!
//! ## Corpus-level files
//!
//! Reserved names start with `_` and never collide with sources:
//!
//! * `_index.json`: every document and fragment with its participants and
//!   the calls it makes, each call linked to the fragment that *expands* it.
//!   This is the merge map for an orchestrator: to inline `A`'s call into
//!   `B`, look up the call's `expands` fragment.
//! * `_model.json`: the full [`Codebase`] (symbols, spans, signatures, docs,
//!   relations with confidence).
//! * `_overview.md`: cross-cutting views: crate/repository dependency graph,
//!   per-crate module graphs, data models (`erDiagram`) and trait maps.
//! * `_README.md`: how to read all of the above, for agents.
//!
//! ## Stable ids
//!
//! Every symbol is cited by its `sym:` id (`sym:cargo shop . db/Db#insert().`,
//! see [`sealmap_model::sym`]) in headings, front matter and the JSON files.
//! Every node, class, entity and participant id is derived from that id by
//! an injective encoding (`shop__db___tDb___finsert`, see
//! `sealmap_mermaid::Ident::from_symbol`), so the same symbol has the same
//! id in every diagram of every document, no two symbols share one, and
//! diagrams can be concatenated, merged or diffed by id. [`generate`] asserts
//! that uniqueness over the whole codebase.
//!
//! ## Example
//!
//! ```
//! use sealmap_corpus::{CorpusOptions, generate};
//! use sealmap_model::SourceSet;
//! use sealmap_rust::{RustOptions, extract};
//!
//! let mut src = SourceSet::new();
//! src.insert("src/lib.rs", "pub struct A; impl A { pub fn go(&self) { helper(); } }\nfn helper() {}").unwrap();
//! let model = extract(&src, &RustOptions { name: "demo".into(), ..Default::default() }).codebase;
//!
//! let corpus = generate(&model, &CorpusOptions::default());
//! let doc = corpus.document("src/lib.rs.md").unwrap();
//! assert!(doc.contains("sequenceDiagram"));
//! assert!(doc.contains("demo___tA->>demo: helper()"));
//! assert!(corpus.document("_index.json").is_some());
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod contract;
mod document;
mod index;
mod naming;
mod overview;
pub mod pack;
pub mod review;
pub mod route;
pub mod seal;
mod sequence;
mod structure;
pub mod tiers;

use std::collections::BTreeMap;

use sealmap_model::{Codebase, SourcePath, Symbol, SymbolId};

pub use contract::{Drift, DriftEntry, Report, corpus_hash, read_dir_corpus, verify, verify_against, write};
pub use index::{CallRef, DocumentEntry, FragmentEntry, FragmentKind, Index};

/// Version of the corpus layout, front-matter and `_index.json` schema.
///
/// | Version | Change |
/// |---|---|
/// | 1 | `::` path ids, field `schema` (sealmap 0.1) |
/// | 2 | `sym:` ids, quoted `module:` front matter, fragment `sig_hash` / `body_hash`, field `schema_version` |
pub const CORPUS_SCHEMA_VERSION: u32 = 2;

/// Suffix appended to a source path to name its document.
pub const DOC_SUFFIX: &str = ".md";

/// How external callees appear as sequence-diagram participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExternalLanes {
    /// One lane per external crate (`tokio`, `serde_json`). Densest.
    #[default]
    CrateRoot,
    /// One lane per external owner (`tokio::fs`, `reqwest::Client`).
    Owner,
}

/// Options controlling projection. Every option is part of the output's
/// identity: changing one changes the documents, which `verify` reports as
/// [`Drift::Modified`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusOptions {
    /// Minimum number of calls a function must make to get a sequence
    /// diagram. Default 1.
    pub min_calls: usize,
    /// Cap on messages per sequence diagram; the rest is summarised in a
    /// note and remains fully listed in `_index.json`. Default 80.
    pub max_messages: usize,
    /// Cap on edges per overview flowchart (Mermaid's default limit is 500).
    /// Heaviest edges are kept. Default 300.
    pub max_edges: usize,
    /// Cap on entities per data-model diagram (highest-degree first).
    /// Default 60.
    pub max_entities: usize,
    /// Include private items in structure diagrams. Default true.
    pub include_private: bool,
    /// External participant grouping.
    pub external_lanes: ExternalLanes,
    /// Also write `_model.json`. Default true.
    pub emit_model: bool,
    /// Pretty-print the JSON files (larger, friendlier diffs). Default false:
    /// compact JSON is roughly half the size.
    pub pretty_json: bool,
}

impl Default for CorpusOptions {
    fn default() -> Self {
        Self {
            min_calls: 1,
            max_messages: 80,
            max_edges: 300,
            max_entities: 60,
            include_private: true,
            external_lanes: ExternalLanes::CrateRoot,
            emit_model: true,
            pretty_json: false,
        }
    }
}

/// A generated corpus: relative output path → file contents, plus the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    /// All files, keyed by path relative to the corpus root.
    pub files: BTreeMap<SourcePath, String>,
    /// The fragment index (also serialised as `_index.json`).
    pub index: Index,
}

impl Corpus {
    /// Contents of a generated file by relative path.
    pub fn document(&self, path: &str) -> Option<&str> {
        SourcePath::new(path).ok().and_then(|p| self.files.get(&p)).map(String::as_str)
    }
}

/// Generate the full corpus in memory. Pure and deterministic: equal inputs
/// give byte-identical output.
pub fn generate(codebase: &Codebase, options: &CorpusOptions) -> Corpus {
    let mut files = BTreeMap::new();
    naming::assert_unique_idents(codebase);
    let mut index = Index::new(codebase);
    let lookup = Lookup::new(codebase);
    for file in codebase.files.values() {
        let (path, text, entry) = document::render(codebase, &lookup, file, options);
        index.documents.push(entry);
        files.insert(path, text);
    }
    index.link_expansions();
    let reserved = |name: &str| SourcePath::new(name).expect("reserved names are valid paths");
    files.insert(reserved("_overview.md"), overview::render(codebase, options));
    files.insert(reserved("_README.md"), readme());
    if options.emit_model {
        files.insert(reserved("_model.json"), to_json(codebase, options.pretty_json));
    }
    files.insert(reserved("_index.json"), to_json(&index, options.pretty_json));
    Corpus { files, index }
}

/// Per-file and per-parent symbol lists, built once per [`generate`] call.
///
/// `Codebase::symbols_in_file` and `Codebase::children` scan every symbol;
/// calling them per document made projection O(files × symbols).
pub(crate) struct Lookup<'a> {
    by_file: BTreeMap<&'a SourcePath, Vec<&'a Symbol>>,
    children: BTreeMap<&'a SymbolId, Vec<&'a Symbol>>,
}

impl<'a> Lookup<'a> {
    fn new(cb: &'a Codebase) -> Self {
        let mut by_file: BTreeMap<&SourcePath, Vec<&Symbol>> = BTreeMap::new();
        let mut children: BTreeMap<&SymbolId, Vec<&Symbol>> = BTreeMap::new();
        // `symbols` iterates in id order, so each list keeps the order the
        // linear scans produced.
        for s in cb.symbols.values() {
            by_file.entry(&s.file).or_default().push(s);
            if let Some(p) = &s.parent {
                children.entry(p).or_default().push(s);
            }
        }
        Self { by_file, children }
    }

    /// Same items and order as `Codebase::symbols_in_file`.
    pub(crate) fn in_file(&self, path: &SourcePath) -> impl Iterator<Item = &'a Symbol> + '_ {
        self.by_file.get(path).into_iter().flatten().copied()
    }

    /// Same items and order as `Codebase::children`.
    pub(crate) fn children(&self, id: &SymbolId) -> impl Iterator<Item = &'a Symbol> + '_ {
        self.children.get(id).into_iter().flatten().copied()
    }
}

/// Map a source path to its document path (`src/a.rs` → `src/a.rs.md`).
pub fn document_path(source: &SourcePath) -> SourcePath {
    source.with_suffix(DOC_SUFFIX)
}

/// `true` for corpus-level files (`_index.json`, ...).
pub fn is_reserved(path: &SourcePath) -> bool {
    path.file_name().starts_with('_') && !path.as_str().contains('/')
}

fn to_json(value: &impl serde::Serialize, pretty: bool) -> String {
    let mut s = if pretty { serde_json::to_string_pretty(value) } else { serde_json::to_string(value) }
        .expect("model types always serialise");
    s.push('\n');
    s
}

fn readme() -> String {
    include_str!("corpus_readme.md").to_owned()
}

/// Compiles and runs the README examples as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
