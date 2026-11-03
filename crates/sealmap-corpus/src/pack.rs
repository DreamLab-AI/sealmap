//! Review packs: one deterministic, bounded text holding chosen topics, the
//! dense slice of the code each one cites and a window of that code's
//! source. An outside reviewer, a seal review or a debugging session reads
//! the pack instead of the repository.
//!
//! Everything here is pure. The caller passes the model, the source texts,
//! the topic texts and an opaque revision string; nothing reads the file
//! system, runs `git` or talks to a network, and identical inputs give
//! byte-identical output. The `sealmap pack` command does the IO.
//!
//! # Format
//!
//! A pack is a header followed by one section per topic, in topic-id order:
//!
//! ```text
//! # sealmap-pack 2
//! generator: sealmap 0.1.0
//! codebase: ledger
//! revision: 4f1a9de…
//! topics: LED-01 LED-02
//! budget: 65536 bytes
//! depth: 1
//! source-window: 40 lines
//! source-inputs: blake3:<digest of all normalised extraction inputs>
//! ==== topic LED-01 312 bytes ledger/01-accounts.md
//! <the topic file, verbatim>
//! ==== dense LED-01 depth 1 845 bytes
//! <the sealmap-dense slice around the topic's cited symbols>
//! ==== unresolved LED-01 61 bytes
//! absent sym:cargo ledger . accounts/LedgerAccount#credit().
//! ==== source LED-01 src/accounts.rs:L4-6 49 bytes sym:cargo ledger . accounts/LedgerAccount#deposit().
//! <lines 4 to 6 of src/accounts.rs, verbatim>
//! ==== end LED-01
//! ```
//!
//! * Every block line starts with `==== ` and gives the byte length of the
//!   payload that follows it, so a reader can skip a block exactly even when
//!   a topic's own text contains such a line. A payload that does not end in
//!   a newline is followed by one that the length does not count.
//! * The header records everything that shapes the output: the generator,
//!   the codebase name, the revision (and the `diff:` base when topics were
//!   chosen by change), the topics, the budget, the slice depth and the
//!   source window. A shard adds `shard: N`.
//! * **Citations** are the topic's `sym:` code spans
//!   ([`citations`]), deduplicated and in id order.
//!   The ones in the model seed the dense slice
//!   ([`sealmap_dense::Dense::slice`]) and each gets a source window at its
//!   current span. The rest are listed under `unresolved` as `absent`,
//!   `unparsable` (with the file) or `invalid` (with the reason), and are
//!   never silently left out.
//! * **Source windows** show at most [`PackOptions::source_window`] lines
//!   of each cited symbol, from its first line. A clipped window says so in
//!   its label: `src/a.rs:L10-49 of L10-80`.
//!
//! # Budget
//!
//! With [`PackOptions::budget`] set, a pack longer than the budget is
//! refused with [`PackError::OverBudget`], which names the header's size and
//! each topic section's size so the caller can plan shards. Nothing is ever
//! truncated. [`shard`] does that planning: it fills numbered packs with
//! whole topics, in topic order, and refuses only a topic too large to fit a
//! pack on its own.
//!
//! # Example
//!
//! ```
//! use sealmap_corpus::pack::{PackError, PackInput, PackOptions, pack};
//! use sealmap_corpus::seal::Topics;
//! use sealmap_model::{SourcePath, SourceSet};
//! use sealmap_rust::{RustOptions, extract};
//!
//! let mut src = SourceSet::new();
//! src.insert("src/lib.rs", "pub struct Ledger;\nimpl Ledger {\n    pub fn post(&self) {}\n}\n").unwrap();
//! let model = extract(&src, &RustOptions { name: "shop".into(), ..Default::default() }).codebase;
//! let mut topics = Topics::new();
//! topics.insert(
//!     SourcePath::new("ledger/01-posting.md").unwrap(),
//!     "---\nid: LED-01\n---\nPosting goes through `sym:cargo shop . Ledger#post().`.\n".into(),
//! );
//!
//! let input = PackInput::new(&model, &src, &topics, "4f1a9de");
//! let ids = ["LED-01".to_string()];
//! let out = pack(&input, &ids, &PackOptions::default()).unwrap();
//! assert!(out.text.starts_with("# sealmap-pack 2\n"));
//! assert!(out.text.contains("==== source LED-01 src/lib.rs:L3-3 26 bytes sym:cargo shop . Ledger#post().\n    pub fn post(&self) {}\n"));
//! // Over budget: refused, with the sizes a caller needs to shard.
//! let err = pack(&input, &ids, &PackOptions::default().budget(100)).unwrap_err();
//! assert!(matches!(err, PackError::OverBudget { budget: 100, .. }));
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use sealmap_dense::{Dense, SliceOptions};
use sealmap_model::{Codebase, ContentHash, Fingerprint, SourcePath, SourceSet, SymbolId};
use serde::Serialize;

use crate::seal::{GENERATOR, Lock, Resolution, Topics, citations, resolve, stale, topic_id};

mod selection;
pub use selection::changed_topics_by_files;

/// Version of the pack format, written in its first line. Bumped whenever a
/// block's layout or meaning changes.
pub const PACK_FORMAT_VERSION: u32 = 2;

/// How a pack is built. Every field is recorded in the pack's header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackOptions {
    /// Call levels each topic's dense slice follows from its cited symbols,
    /// towards callees and callers. Default 1.
    pub depth: usize,
    /// Most source lines shown per cited symbol; 0 shows none. Default 40.
    pub source_window: usize,
    /// Refuse a pack longer than this many bytes. `None` (the default): no
    /// limit.
    pub budget: Option<usize>,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self { depth: 1, source_window: 40, budget: None }
    }
}

impl PackOptions {
    /// The same options with a slice depth.
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// The same options with a source window of `lines` lines per symbol.
    pub fn source_window(mut self, lines: usize) -> Self {
        self.source_window = lines;
        self
    }

    /// The same options with a byte budget.
    pub fn budget(mut self, bytes: usize) -> Self {
        self.budget = Some(bytes);
        self
    }
}

/// What a pack is built from.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct PackInput<'a> {
    /// The model of the current code.
    pub codebase: &'a Codebase,
    /// The source texts the model was extracted from, for source windows.
    pub sources: &'a SourceSet,
    /// The authored topics: path relative to the topic directory → text.
    pub topics: &'a Topics,
    /// The revision the code is at, recorded as given (for example a git
    /// commit, with `+dirty` for a modified working tree).
    pub revision: &'a str,
    /// The base revision when the topics were chosen by change
    /// ([`changed_topics`]), recorded as `diff:`.
    pub diff: Option<&'a str>,
    /// Optional deterministic provenance supplied by the IO layer, normally a
    /// JSON map of repository names to commits and dirty/untracked input paths.
    /// This is descriptive metadata, not a claim that the review is correct.
    pub provenance: Option<&'a str>,
}

impl<'a> PackInput<'a> {
    /// An input with no `diff:` base.
    pub fn new(codebase: &'a Codebase, sources: &'a SourceSet, topics: &'a Topics, revision: &'a str) -> Self {
        Self { codebase, sources, topics, revision, diff: None, provenance: None }
    }

    /// The same input, recording the revision the topics were chosen against.
    pub fn diff(mut self, base: &'a str) -> Self {
        self.diff = Some(base);
        self
    }

    /// Include the IO layer's repository provenance in every pack header.
    pub fn provenance(mut self, provenance: &'a str) -> Self {
        self.provenance = Some(provenance);
        self
    }
}

/// The size of one topic's section.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct TopicSize {
    /// The topic id.
    pub id: String,
    /// Bytes of its section, from `==== topic` to `==== end` inclusive.
    pub bytes: usize,
}

/// A built pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// The pack text.
    pub text: String,
    /// Bytes of the header.
    pub header: usize,
    /// Each topic's section size, in pack order.
    pub topics: Vec<TopicSize>,
}

/// Why [`pack`] or [`shard`] refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PackError {
    /// No topic was asked for.
    NoTopics,
    /// Requested topic ids that no topic carries, in id order.
    UnknownTopics {
        /// The unknown ids.
        ids: Vec<String>,
    },
    /// Two topic files carry the same id, so the id names no single topic.
    DuplicateTopic {
        /// The id.
        id: String,
        /// The files carrying it, in path order.
        files: Vec<SourcePath>,
    },
    /// The pack is longer than the budget. Nothing was truncated.
    OverBudget {
        /// Bytes of the whole pack.
        bytes: usize,
        /// The budget it exceeded.
        budget: usize,
        /// Bytes of the header.
        header: usize,
        /// Each topic's section size, in pack order.
        topics: Vec<TopicSize>,
    },
    /// One topic does not fit a pack of the budget even on its own, so no
    /// sharding can hold it.
    TopicOverBudget {
        /// The topic id.
        id: String,
        /// Bytes of a pack holding only that topic.
        bytes: usize,
        /// The budget.
        budget: usize,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTopics => write!(f, "no topic to pack"),
            Self::UnknownTopics { ids } => write!(f, "no topic has the id(s) {}", ids.join(", ")),
            Self::DuplicateTopic { id, files } => {
                let files: Vec<&str> = files.iter().map(SourcePath::as_str).collect();
                write!(f, "topic id {id} is carried by {} files: {}", files.len(), files.join(", "))
            }
            Self::OverBudget { bytes, budget, header, topics } => {
                write!(
                    f,
                    "pack is {bytes} bytes, {} over the {budget}-byte budget; refusing rather than truncating (header {header}",
                    bytes - budget
                )?;
                for t in topics {
                    write!(f, ", {} {}", t.id, t.bytes)?;
                }
                write!(f, ")")
            }
            Self::TopicOverBudget { id, bytes, budget } => {
                write!(f, "topic {id} alone makes a {bytes}-byte pack, over the {budget}-byte budget")
            }
        }
    }
}

impl std::error::Error for PackError {}

/// Build one pack of the topics `ids` (topic ids, in any order; duplicates
/// count once).
///
/// # Errors
///
/// [`PackError::NoTopics`], [`PackError::UnknownTopics`] and
/// [`PackError::DuplicateTopic`] for a bad selection, and
/// [`PackError::OverBudget`] when the pack exceeds
/// [`PackOptions::budget`].
pub fn pack(input: &PackInput<'_>, ids: &[String], options: &PackOptions) -> Result<Pack, PackError> {
    let builder = Builder::new(input, ids, options)?;
    let sections = builder.sections();
    let pack = builder.assemble(&sections, None);
    match options.budget {
        Some(budget) if pack.text.len() > budget => {
            Err(PackError::OverBudget { bytes: pack.text.len(), budget, header: pack.header, topics: pack.topics })
        }
        _ => Ok(pack),
    }
}

/// Build packs of the topics `ids` that each fit [`PackOptions::budget`]:
/// whole topics, in topic-id order, each pack filled before the next is
/// started. Each pack's header names its topics and its shard number,
/// counted from 1. Without a budget this is one pack, as [`pack`] builds it.
///
/// # Errors
///
/// As [`pack`] for a bad selection, and [`PackError::TopicOverBudget`] when
/// one topic does not fit a pack on its own.
pub fn shard(input: &PackInput<'_>, ids: &[String], options: &PackOptions) -> Result<Vec<Pack>, PackError> {
    let builder = Builder::new(input, ids, options)?;
    let sections = builder.sections();
    let Some(budget) = options.budget else { return Ok(vec![builder.assemble(&sections, None)]) };
    // Sizes are planned from the header and the section lengths; each pack
    // is assembled once.
    let mut plan: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        let size = |members: &[usize], shard: usize| {
            let ids: Vec<&str> = members.iter().map(|&m| sections[m].id.as_str()).collect();
            builder.header(&ids, Some(shard)).len() + members.iter().map(|&m| sections[m].text.len()).sum::<usize>()
        };
        current.push(i);
        if size(&current, plan.len() + 1) <= budget {
            continue;
        }
        current.pop();
        if !current.is_empty() {
            plan.push(std::mem::take(&mut current));
        }
        let alone = size(&[i], plan.len() + 1);
        if alone > budget {
            return Err(PackError::TopicOverBudget { id: section.id.clone(), bytes: alone, budget });
        }
        current.push(i);
    }
    if !current.is_empty() {
        plan.push(current);
    }
    Ok(plan
        .iter()
        .enumerate()
        .map(|(n, members)| {
            let chosen: Vec<Section> = members.iter().map(|&m| sections[m].clone()).collect();
            builder.assemble(&chosen, Some(n + 1))
        })
        .collect())
}

/// The topics whose sealed or cited symbols changed between `before` and
/// `after`, in id order: the topics [`stale`] reports for `lock` against
/// `before`, plus every topic citing a symbol that was added, removed, made
/// unparsable or given a different signature or body hash. A symbol that
/// only moved has not changed.
///
/// ```
/// use sealmap_corpus::pack::changed_topics;
/// use sealmap_corpus::seal::Topics;
/// use sealmap_model::{SourcePath, SourceSet};
/// use sealmap_rust::{RustOptions, extract};
///
/// let model = |body: &str| {
///     let mut src = SourceSet::new();
///     src.insert("src/lib.rs", format!("pub fn post() {{ {body} }}\npub fn void() {{}}")).unwrap();
///     extract(&src, &RustOptions { name: "shop".into(), ..Default::default() }).codebase
/// };
/// let mut topics = Topics::new();
/// topics.insert(SourcePath::new("a.md").unwrap(), "---\nid: A-01\n---\n`sym:cargo shop . post().`\n".into());
/// topics.insert(SourcePath::new("b.md").unwrap(), "---\nid: B-01\n---\n`sym:cargo shop . void().`\n".into());
/// assert_eq!(changed_topics(None, &topics, &model(""), &model("void();")), ["A-01"]);
/// ```
pub fn changed_topics(lock: Option<&Lock>, topics: &Topics, before: &Codebase, after: &Codebase) -> Vec<String> {
    let mut out: BTreeSet<String> =
        lock.map(|l| stale(l, Some(before), after).into_iter().map(|s| s.topic).collect()).unwrap_or_default();
    for text in topics.values() {
        let Some(id) = topic_id(text) else { continue };
        let changed = citations(text)
            .iter()
            .filter_map(|c| c.id.as_ref().ok())
            .any(|sym| SymbolState::of(before, sym) != SymbolState::of(after, sym));
        if changed {
            out.insert(id);
        }
    }
    out.into_iter().collect()
}

/// What [`changed_topics`] compares: where a symbol is does not count.
#[derive(Debug, PartialEq, Eq)]
enum SymbolState {
    Found(Fingerprint, Fingerprint),
    Unparsable,
    Absent,
}

impl SymbolState {
    fn of(codebase: &Codebase, id: &SymbolId) -> Self {
        match resolve(codebase, id, None) {
            Resolution::Found { sig, body, .. } => Self::Found(sig, body),
            Resolution::Unparsable { .. } => Self::Unparsable,
            Resolution::Absent { .. } => Self::Absent,
        }
    }
}

/// One topic's rendered section.
#[derive(Debug, Clone)]
struct Section {
    id: String,
    text: String,
}

/// A validated selection, ready to render.
struct Builder<'a, 'i> {
    input: &'i PackInput<'a>,
    options: &'i PackOptions,
    /// Selected topics: id → (file, text), in id order.
    selected: BTreeMap<String, (&'a SourcePath, &'a str)>,
    source_hash: ContentHash,
}

impl<'a, 'i> Builder<'a, 'i> {
    fn new(input: &'i PackInput<'a>, ids: &[String], options: &'i PackOptions) -> Result<Self, PackError> {
        if ids.is_empty() {
            return Err(PackError::NoTopics);
        }
        let mut by_id: BTreeMap<String, Vec<(&'a SourcePath, &'a str)>> = BTreeMap::new();
        for (path, text) in input.topics {
            if let Some(id) = topic_id(text) {
                by_id.entry(id).or_default().push((path, text.as_str()));
            }
        }
        let wanted: BTreeSet<&String> = ids.iter().collect();
        let unknown: Vec<String> =
            wanted.iter().filter(|id| !by_id.contains_key(id.as_str())).map(|id| (*id).clone()).collect();
        if !unknown.is_empty() {
            return Err(PackError::UnknownTopics { ids: unknown });
        }
        let mut selected = BTreeMap::new();
        for id in wanted {
            let files = &by_id[id.as_str()];
            if files.len() > 1 {
                return Err(PackError::DuplicateTopic {
                    id: id.clone(),
                    files: files.iter().map(|(p, _)| (*p).clone()).collect(),
                });
            }
            selected.insert(id.clone(), files[0]);
        }
        // Length-prefix both path and text, so file boundaries cannot alias.
        // Hash once: shard planning calls header repeatedly.
        let mut hash = blake3::Hasher::new();
        for (path, text) in input.sources.iter() {
            hash.update(&(path.as_str().len() as u64).to_le_bytes());
            hash.update(path.as_str().as_bytes());
            hash.update(&(text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        let source_hash = ContentHash::parse(&format!("blake3:{}", hash.finalize().to_hex()))
            .expect("BLAKE3 emits a valid full hash");
        Ok(Self { input, options, selected, source_hash })
    }

    fn sections(&self) -> Vec<Section> {
        let dense = Dense::new(self.input.codebase);
        self.selected
            .iter()
            .map(|(id, (file, text))| Section { id: id.clone(), text: self.section(&dense, id, file, text) })
            .collect()
    }

    fn section(&self, dense: &Dense<'_>, id: &str, file: &SourcePath, text: &str) -> String {
        let cb = self.input.codebase;
        let mut found: BTreeSet<&SymbolId> = BTreeSet::new();
        let mut unresolved = String::new();
        let mut seen = BTreeSet::new();
        let mut cited: Vec<_> = citations(text);
        cited.sort_by(|a, b| a.text.cmp(&b.text));
        for c in &cited {
            if !seen.insert(c.text.as_str()) {
                continue;
            }
            match &c.id {
                Err(e) => {
                    let _ = writeln!(unresolved, "invalid {}: {e}", c.text);
                }
                Ok(sym) => match resolve(cb, sym, None) {
                    Resolution::Found { .. } => {
                        if let Some((key, _)) = cb.symbols.get_key_value(sym) {
                            found.insert(key);
                        }
                    }
                    Resolution::Unparsable { file } => {
                        let _ = writeln!(unresolved, "unparsable {sym} {file}");
                    }
                    Resolution::Absent { .. } => {
                        let _ = writeln!(unresolved, "absent {sym}");
                    }
                },
            }
        }

        let mut out = String::new();
        block(&mut out, &format!("topic {id}"), Some(file.as_str()), text);
        if !found.is_empty() {
            let slice = dense
                .slice(found.iter().copied(), &SliceOptions::new(self.options.depth))
                .expect("seeds are symbols of the codebase and no budget is set");
            block(&mut out, &format!("dense {id} depth {}", self.options.depth), None, &slice);
        }
        if !unresolved.is_empty() {
            block(&mut out, &format!("unresolved {id}"), None, &unresolved);
        }
        if self.options.source_window > 0 {
            for sym in &found {
                if let Some(s) = cb.symbol(sym) {
                    let (label, window) = self.window(&s.file, s.span.start_line, s.span.end_line);
                    block(&mut out, &format!("source {id} {label}"), Some(sym.as_str()), &window);
                }
            }
        }
        let _ = writeln!(out, "==== end {id}");
        out
    }

    /// The label and text of the source window for lines `start..=end` of
    /// `file`, clipped to the option's line count and to the file.
    fn window(&self, file: &SourcePath, start: u32, end: u32) -> (String, String) {
        let Some(text) = self.input.sources.get(file) else {
            return (format!("{file}:L{start}-{end} (no source text)"), String::new());
        };
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let start = start.max(1) as usize;
        let end = (end as usize).max(start);
        let shown_end = end.min(start + self.options.source_window - 1).min(lines.len());
        if start > lines.len() {
            return (format!("{file}:L{start}-{end} (past the end of the file)"), String::new());
        }
        let label = if shown_end < end {
            format!("{file}:L{start}-{shown_end} of L{start}-{end}")
        } else {
            format!("{file}:L{start}-{end}")
        };
        (label, lines[start - 1..shown_end].concat())
    }

    /// The header plus `sections`, as one pack (shard `shard` when given).
    fn assemble(&self, sections: &[Section], shard: Option<usize>) -> Pack {
        let ids: Vec<&str> = sections.iter().map(|s| s.id.as_str()).collect();
        let mut text = self.header(&ids, shard);
        let header = text.len();
        let mut topics = Vec::with_capacity(sections.len());
        for s in sections {
            text.push_str(&s.text);
            topics.push(TopicSize { id: s.id.clone(), bytes: s.text.len() });
        }
        Pack { text, header, topics }
    }

    /// The header of a pack of the topics `ids` (shard `shard` when given).
    fn header(&self, ids: &[&str], shard: Option<usize>) -> String {
        let input = self.input;
        let mut text = format!("# sealmap-pack {PACK_FORMAT_VERSION}\ngenerator: {GENERATOR}\n");
        let _ = writeln!(text, "codebase: {}", one_line(&input.codebase.name));
        let _ = writeln!(text, "revision: {}", one_line(input.revision));
        if let Some(base) = input.diff {
            let _ = writeln!(text, "diff: {}", one_line(base));
        }
        let _ = writeln!(text, "topics: {}", ids.join(" "));
        match self.options.budget {
            Some(b) => {
                let _ = writeln!(text, "budget: {b} bytes");
            }
            None => text.push_str("budget: none\n"),
        }
        let _ = writeln!(text, "depth: {}", self.options.depth);
        let _ = writeln!(text, "source-window: {} lines", self.options.source_window);
        let _ = writeln!(text, "source-inputs: {}", self.source_hash);
        if let Some(provenance) = input.provenance {
            let _ = writeln!(text, "provenance: {}", one_line(provenance));
        }
        if let Some(n) = shard {
            let _ = writeln!(text, "shard: {n}");
        }
        text
    }
}

/// Write one block: its `==== ` line with the payload's byte length (and a
/// trailing free-text field, which may hold spaces), then the payload, then
/// a newline if the payload does not end in one.
fn block(out: &mut String, head: &str, tail: Option<&str>, payload: &str) {
    let _ = write!(out, "==== {head} {} bytes", payload.len());
    if let Some(tail) = tail {
        let _ = write!(out, " {}", one_line(tail));
    }
    out.push('\n');
    out.push_str(payload);
    if !payload.is_empty() && !payload.ends_with('\n') {
        out.push('\n');
    }
}

/// `s` with line breaks replaced, so a header field stays on its line.
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_count_payload_bytes_only() {
        let mut out = String::new();
        block(&mut out, "topic A-01", Some("a b.md"), "x\ny");
        assert_eq!(out, "==== topic A-01 3 bytes a b.md\nx\ny\n");
        let mut out = String::new();
        block(&mut out, "unresolved A-01", None, "absent x\n");
        assert_eq!(out, "==== unresolved A-01 9 bytes\nabsent x\n");
    }
}
