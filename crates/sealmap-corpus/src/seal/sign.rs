//! The write path: derive a topic's seal from the current code.

use std::collections::BTreeMap;

use sealmap_model::{Codebase, SourcePath, SymbolId};

use super::check::{Resolution, resolve};
use super::lock::{GENERATOR, Lock, SymbolSeal, TopicSeal, is_date};
use super::topic::{citations, topic_hash, topic_id, with_pointer};

/// Who and when, for [`sign`]. All three are recorded as given. `reviewer`
/// and `model` are `host:model` routes (see [`route`](crate::route)); `sign`
/// refuses a pair that shares a model family or whose family is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The route of the reviewer who checked the topic against the code.
    pub reviewer: String,
    /// The route of the model that wrote the topic.
    pub model: String,
    /// The seal date, `YYYY-MM-DD`.
    pub date: String,
}

/// Why [`sign`] refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignError {
    /// The topic has no front matter, or no `id:` in it.
    #[error("{0}: no front matter with an `id:`")]
    NoTopicId(SourcePath),
    /// The topic id is not usable as a lock key.
    #[error("{0}: topic id `{1}` is empty or contains control characters")]
    BadTopicId(SourcePath, String),
    /// Another lock entry already uses this topic id for a different file.
    #[error("topic id `{id}` is already sealed for {other}")]
    IdTaken {
        /// The id.
        id: String,
        /// The file the existing entry names.
        other: SourcePath,
    },
    /// A citation does not parse as a `sym:` id.
    #[error("line {line}: `{text}` is not a canonical sym: id")]
    Malformed {
        /// The 1-based line.
        line: u32,
        /// The cited text.
        text: String,
    },
    /// A cited id cannot be sealed: it is not in the model, a file that may
    /// hold it does not parse, or it has no fingerprints.
    #[error("line {line}: `{id}` cannot be sealed: {why}")]
    Unsealable {
        /// The 1-based line.
        line: u32,
        /// The cited id.
        id: SymbolId,
        /// The reason.
        why: String,
    },
    /// `reviewer` or `model` is empty, or `date` is not `YYYY-MM-DD`.
    #[error("{0}")]
    BadSignature(String),
    /// The reviewer shares the author's model family, or a family is
    /// unknown (ADR-2130). Nothing is sealed.
    #[error("{0}")]
    Family(#[from] crate::route::RouteError),
}

/// Seal the topic at `file` (relative to the lock's directory) with text
/// `text`: derive its entry mechanically from `codebase` and insert it into
/// `lock`, replacing any previous entry for the same topic. Returns the topic
/// text with its `sealed: <lock_name>` pointer in place (unchanged if it was
/// already there), which the caller writes back together with
/// [`Lock::to_toml`].
///
/// The entry holds every `sym:` id the topic cites (see
/// [`citations`](super::citations)), each with its current `sig_hash` and
/// `body_hash`, and the [`topic_hash`](super::topic_hash) of the text. A
/// citation that does not parse, is not in the model, may sit in an
/// unparsable file, or has no fingerprints is refused: nothing is sealed
/// that `verify` could not then confirm. The reviewer must be a different
/// model family from `who.model`, else [`SignError::Family`]. Signing records `lock.generator`
/// as this build.
///
/// ```
/// use sealmap_corpus::seal::{Lock, Signature, sign, verify};
/// use sealmap_model::SourceSet;
/// use sealmap_rust::{RustOptions, extract};
///
/// let mut src = SourceSet::new();
/// src.insert("src/lib.rs", "pub struct Ledger; impl Ledger { pub fn post(&self) {} }").unwrap();
/// let model = extract(&src, &RustOptions { name: "books".into(), ..Default::default() }).codebase;
///
/// let file = sealmap_model::SourcePath::new("core/01-ledger.md").unwrap();
/// let topic = "---\nid: CORE-01\n---\nPosting is `sym:cargo books . Ledger#post().`.\n";
/// let who = Signature { reviewer: "zai:glm-5.3".into(), model: "claude:claude-sonnet-5-5".into(), date: "2026-10-05".into() };
///
/// let mut lock = Lock::new();
/// let sealed_text = sign(&mut lock, &file, topic, &model, "seals.lock", &who).unwrap();
/// assert!(sealed_text.contains("\nsealed: seals.lock\n"));
///
/// let topics = [(file, sealed_text)].into_iter().collect();
/// assert!(verify(Some(&lock.to_toml()), "seals.lock", &topics, &model).passes());
/// ```
pub fn sign(
    lock: &mut Lock,
    file: &SourcePath,
    text: &str,
    codebase: &Codebase,
    lock_name: &str,
    who: &Signature,
) -> Result<String, SignError> {
    if who.reviewer.trim().is_empty() || who.model.trim().is_empty() {
        return Err(SignError::BadSignature("reviewer and model must not be empty".into()));
    }
    if !is_date(&who.date) {
        return Err(SignError::BadSignature(format!("date `{}` is not YYYY-MM-DD", who.date)));
    }
    crate::route::cross_family(&who.model, &who.reviewer)?;
    let id = topic_id(text).ok_or_else(|| SignError::NoTopicId(file.clone()))?;
    if !super::lock::is_topic_id(&id) {
        return Err(SignError::BadTopicId(file.clone(), id));
    }
    if let Some(other) = lock.topics.get(&id).filter(|t| &t.file != file) {
        return Err(SignError::IdTaken { id, other: other.file.clone() });
    }
    let mut symbols = BTreeMap::new();
    for c in citations(text) {
        let sym = c.id.map_err(|_| SignError::Malformed { line: c.line, text: c.text.clone() })?;
        let unsealable = |why: String| SignError::Unsealable { line: c.line, id: sym.clone(), why };
        match resolve(codebase, &sym, None) {
            Resolution::Found { sig, body, .. } if !sig.is_unset() && !body.is_unset() => {
                symbols.insert(sym, SymbolSeal { sig, body });
            }
            Resolution::Found { .. } => return Err(unsealable("it has no fingerprints".into())),
            Resolution::Unparsable { file } => return Err(unsealable(format!("{file} does not parse"))),
            Resolution::Absent { .. } => return Err(unsealable("it is not in the model".into())),
        }
    }
    let sealed_text = with_pointer(text, lock_name).ok_or_else(|| SignError::NoTopicId(file.clone()))?;
    // A file can be sealed under one id only.
    lock.topics.retain(|_, t| &t.file != file);
    lock.generator = GENERATOR.into();
    lock.insert(TopicSeal {
        id,
        file: file.clone(),
        topic_hash: topic_hash(&sealed_text),
        reviewer: who.reviewer.clone(),
        model: who.model.clone(),
        date: who.date.clone(),
        symbols,
    });
    Ok(sealed_text)
}
