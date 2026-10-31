//! `seals.lock`: the parsed form and its one canonical serialisation.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use sealmap_model::{Fingerprint, SourcePath, SymbolId};
use serde::{Deserialize, Serialize};

/// The lock format version this build reads and writes.
pub const LOCK_VERSION: u32 = 1;

/// The fingerprint algorithm every hash in a lock was computed with: the
/// `sm1` token-stream fingerprints of `sealmap-extract` for `sig` and
/// `body`, and [`topic_hash`](super::topic_hash) for the prose.
pub const ALGORITHM: &str = "sm1";

/// The conventional file name of a lock, next to the topics it seals.
pub const LOCK_FILE: &str = "seals.lock";

/// The `generator` value this build writes.
pub const GENERATOR: &str = concat!("sealmap ", env!("CARGO_PKG_VERSION"));

/// A whole `seals.lock`.
///
/// The canonical text ([`Lock::to_toml`]) is the only form a lock may take on
/// disk: [`Lock::parse_canonical`] refuses anything else, and the seal check
/// reports a non-canonical lock as a lock fault. Topics are keyed and written
/// in topic-id order and each topic's symbols in id order, so two people
/// sealing different topics produce disjoint hunks and a merge conflict stays
/// within one topic.
///
/// ```
/// use sealmap_corpus::seal::{Lock, SymbolSeal, TopicSeal};
/// use sealmap_model::{Fingerprint, SourcePath, SymbolId};
///
/// let mut lock = Lock::new();
/// let mut topic = TopicSeal {
///     id: "CP-03".into(),
///     file: SourcePath::new("control-plane/03-the-executor.md").unwrap(),
///     topic_hash: Fingerprint::from_bytes([7; 16]),
///     reviewer: "zai:glm-5.3".into(),
///     model: "claude:claude-sonnet-5".into(),
///     date: "2026-10-05".into(),
///     symbols: Default::default(),
/// };
/// let id = SymbolId::parse("sym:cargo shop . db/Db#insert().").unwrap();
/// topic.symbols.insert(id, SymbolSeal { sig: Fingerprint::from_bytes([1; 16]), body: Fingerprint::from_bytes([2; 16]) });
/// lock.insert(topic);
///
/// let text = lock.to_toml();
/// assert!(text.starts_with("version = 1\nalgorithm = \"sm1\"\n"));
/// // The canonical text parses back to the same lock, and is the only text that does.
/// assert_eq!(Lock::parse_canonical(&text).unwrap(), lock);
/// assert!(Lock::parse_canonical(&text.replace("version = 1", "version=1")).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lock {
    /// Format version, always [`LOCK_VERSION`] once parsed.
    pub version: u32,
    /// Fingerprint algorithm, always [`ALGORITHM`] once parsed.
    pub algorithm: String,
    /// The tool that last wrote the lock. Kept as read; [`Lock::new`] and the
    /// sign step set it to [`GENERATOR`].
    pub generator: String,
    /// Sealed topics keyed by topic id.
    pub topics: BTreeMap<String, TopicSeal>,
}

/// One sealed topic: the record that a reviewer checked the topic's claims
/// against these exact symbol versions and this exact prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopicSeal {
    /// The topic's id (its front-matter `id:`).
    pub id: String,
    /// The topic file, relative to the directory holding the lock.
    pub file: SourcePath,
    /// [`topic_hash`](super::topic_hash) of the topic text when sealed.
    pub topic_hash: Fingerprint,
    /// The reviewer's `host:model` route. `sign` guarantees a different model
    /// family from `model`; reading a lock does not re-check it.
    pub reviewer: String,
    /// The `host:model` route of the model that wrote the topic.
    pub model: String,
    /// Seal date, `YYYY-MM-DD`.
    pub date: String,
    /// Sealed symbols with their hashes at sealing time.
    pub symbols: BTreeMap<SymbolId, SymbolSeal>,
}

/// The two fingerprints a symbol had when it was sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SymbolSeal {
    /// The symbol's `sig_hash` (contract).
    pub sig: Fingerprint,
    /// The symbol's `body_hash` (implementation).
    pub body: Fingerprint,
}

/// Why a lock could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LockError {
    /// Not TOML, or not the shape of a lock (unknown or missing keys).
    #[error("not a valid seals.lock: {0}")]
    Syntax(String),
    /// A `version` other than [`LOCK_VERSION`].
    #[error("lock version {found} is not supported (expected {LOCK_VERSION})")]
    Version {
        /// The version found.
        found: i64,
    },
    /// An `algorithm` other than [`ALGORITHM`].
    #[error("lock algorithm `{0}` is not supported (expected `{ALGORITHM}`)")]
    Algorithm(String),
    /// A field holds a value of the wrong form.
    #[error("topic `{topic}`: {message}")]
    Field {
        /// The topic the field belongs to.
        topic: String,
        /// What is wrong.
        message: String,
    },
    /// Two entries share a topic id.
    #[error("topic `{0}` is sealed twice")]
    DuplicateTopic(String),
    /// Two entries name the same file.
    #[error("file `{0}` is sealed by two topics")]
    DuplicateFile(String),
    /// A topic seals the same symbol twice.
    #[error("topic `{topic}` seals `{symbol}` twice")]
    DuplicateSymbol {
        /// The topic.
        topic: String,
        /// The repeated symbol id.
        symbol: String,
    },
    /// The text parses but is not the canonical serialisation.
    #[error("the lock is not in canonical form (rewrite it with `sealmap seal sign`)")]
    NonCanonical,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLock {
    version: i64,
    algorithm: String,
    generator: String,
    #[serde(default)]
    topic: Vec<RawTopic>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTopic {
    id: String,
    file: String,
    topic_hash: String,
    reviewer: String,
    model: String,
    date: String,
    symbols: Vec<RawSymbol>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSymbol {
    id: String,
    sig: String,
    body: String,
}

impl Default for Lock {
    fn default() -> Self {
        Self::new()
    }
}

impl Lock {
    /// An empty lock written by this build.
    pub fn new() -> Self {
        Self {
            version: LOCK_VERSION,
            algorithm: ALGORITHM.into(),
            generator: GENERATOR.into(),
            topics: BTreeMap::new(),
        }
    }

    /// Insert or replace a topic's seal.
    pub fn insert(&mut self, topic: TopicSeal) {
        self.topics.insert(topic.id.clone(), topic);
    }

    /// The seal of the topic whose file is `file`, if any.
    pub fn by_file(&self, file: &SourcePath) -> Option<&TopicSeal> {
        self.topics.values().find(|t| &t.file == file)
    }

    /// Parse a lock, validating every field, without requiring canonical
    /// form. Line endings are normalised first.
    ///
    /// Use this to repair or re-sign a lock; use [`Lock::parse_canonical`]
    /// to check one.
    pub fn parse(text: &str) -> Result<Self, LockError> {
        let text = normalise(text);
        let raw: RawLock = toml::from_str(&text).map_err(|e| LockError::Syntax(one_line(&e.to_string())))?;
        if raw.version != i64::from(LOCK_VERSION) {
            return Err(LockError::Version { found: raw.version });
        }
        if raw.algorithm != ALGORITHM {
            return Err(LockError::Algorithm(raw.algorithm));
        }
        let mut topics = BTreeMap::new();
        let mut files = BTreeMap::new();
        for t in raw.topic {
            let topic = TopicSeal::from_raw(t)?;
            if files.insert(topic.file.clone(), topic.id.clone()).is_some() {
                return Err(LockError::DuplicateFile(topic.file.to_string()));
            }
            if topics.contains_key(&topic.id) {
                return Err(LockError::DuplicateTopic(topic.id));
            }
            topics.insert(topic.id.clone(), topic);
        }
        Ok(Self { version: LOCK_VERSION, algorithm: raw.algorithm, generator: raw.generator, topics })
    }

    /// Parse a lock and require that `text` is byte-for-byte its canonical
    /// serialisation ([`Lock::to_toml`]); anything else is
    /// [`LockError::NonCanonical`]. Line endings are not normalised here: a
    /// lock checked out with `\r\n` is not canonical.
    pub fn parse_canonical(text: &str) -> Result<Self, LockError> {
        let lock = Self::parse(text)?;
        if lock.to_toml() != text {
            return Err(LockError::NonCanonical);
        }
        Ok(lock)
    }

    /// The canonical serialisation.
    ///
    /// The layout is fixed: the three header keys, then one `[[topic]]` table
    /// per topic in id order, separated by blank lines, with the keys `id`,
    /// `file`, `topic_hash`, `reviewer`, `model`, `date` and `symbols` in
    /// that order. `symbols` is a multi-line array with one inline table per
    /// symbol, in id order, each line ending in a comma (`symbols = []` when
    /// empty). Strings are TOML basic strings with the minimal escapes. The
    /// text ends in exactly one newline.
    pub fn to_toml(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "version = {}", self.version);
        let _ = writeln!(s, "algorithm = {}", quote(&self.algorithm));
        let _ = writeln!(s, "generator = {}", quote(&self.generator));
        for t in self.topics.values() {
            s.push_str("\n[[topic]]\n");
            let _ = writeln!(s, "id = {}", quote(&t.id));
            let _ = writeln!(s, "file = {}", quote(t.file.as_str()));
            let _ = writeln!(s, "topic_hash = {}", quote(&t.topic_hash.to_string()));
            let _ = writeln!(s, "reviewer = {}", quote(&t.reviewer));
            let _ = writeln!(s, "model = {}", quote(&t.model));
            let _ = writeln!(s, "date = {}", quote(&t.date));
            if t.symbols.is_empty() {
                s.push_str("symbols = []\n");
            } else {
                s.push_str("symbols = [\n");
                for (id, seal) in &t.symbols {
                    let _ = writeln!(
                        s,
                        "  {{ id = {}, sig = {}, body = {} }},",
                        quote(id.as_str()),
                        quote(&seal.sig.to_string()),
                        quote(&seal.body.to_string())
                    );
                }
                s.push_str("]\n");
            }
        }
        s
    }
}

impl TopicSeal {
    fn from_raw(t: RawTopic) -> Result<Self, LockError> {
        let field = |message: String| LockError::Field { topic: t.id.clone(), message };
        if !is_topic_id(&t.id) {
            return Err(field("`id` is empty or contains a control character".into()));
        }
        let file = SourcePath::new(&t.file).map_err(|e| field(format!("`file`: {e}")))?;
        if file.as_str() != t.file {
            return Err(field(format!("`file` `{}` is not a normalised relative path", t.file)));
        }
        let topic_hash =
            Fingerprint::parse(&t.topic_hash).ok_or_else(|| field(format!("`topic_hash` `{}`", t.topic_hash)))?;
        if !is_date(&t.date) {
            return Err(field(format!("`date` `{}` is not YYYY-MM-DD", t.date)));
        }
        let mut symbols = BTreeMap::new();
        for s in &t.symbols {
            let id = SymbolId::parse(&s.id).map_err(|e| field(format!("symbol `{}`: {e}", s.id)))?;
            let sig =
                Fingerprint::parse(&s.sig).ok_or_else(|| field(format!("symbol `{}`: `sig` `{}`", s.id, s.sig)))?;
            let body =
                Fingerprint::parse(&s.body).ok_or_else(|| field(format!("symbol `{}`: `body` `{}`", s.id, s.body)))?;
            if symbols.insert(id, SymbolSeal { sig, body }).is_some() {
                return Err(LockError::DuplicateSymbol { topic: t.id.clone(), symbol: s.id.clone() });
            }
        }
        Ok(Self { id: t.id, file, topic_hash, reviewer: t.reviewer, model: t.model, date: t.date, symbols })
    }
}

/// A topic id: non-empty, no control characters, no edge whitespace.
pub(crate) fn is_topic_id(s: &str) -> bool {
    !s.is_empty() && s.trim() == s && !s.chars().any(char::is_control)
}

/// `YYYY-MM-DD` with a month in 1..=12 and a day in 1..=31.
pub(crate) fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits =
        |r: std::ops::Range<usize>| b[r.clone()].iter().all(u8::is_ascii_digit).then(|| s[r].parse::<u32>().ok());
    match (digits(0..4), digits(5..7), digits(8..10)) {
        (Some(Some(_)), Some(Some(m)), Some(Some(d))) => (1..=12).contains(&m) && (1..=31).contains(&d),
        _ => false,
    }
}

/// A TOML basic string with the minimal escapes: `"` and `\`, the named
/// control escapes, and `\uXXXX` for every other control character.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn normalise(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains('\r') { text.replace("\r\n", "\n").replace('\r', "\n").into() } else { text.into() }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_round_trips_through_toml() {
        for s in ["plain", "a \"b\" c", "back\\slash", "tab\tnew\nline", "\u{1}\u{7f}", "sym:cargo x . `a``b`#", "ünï"]
        {
            let doc = format!("v = {}\n", quote(s));
            let parsed: BTreeMap<String, String> = toml::from_str(&doc).unwrap();
            assert_eq!(parsed["v"], s, "{doc}");
        }
    }

    #[test]
    fn dates() {
        assert!(is_date("2026-10-05"));
        for bad in ["2026-1-05", "2026-13-01", "2026-00-01", "2026-10-32", "20261005xx", "2026/10/05", "２０２６-10-05"]
        {
            assert!(!is_date(bad), "{bad}");
        }
    }
}
