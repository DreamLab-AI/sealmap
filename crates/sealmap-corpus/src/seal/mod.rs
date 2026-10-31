//! Seals: the record that a hand-written topic was reviewed against exact
//! versions of the code it cites, and the checks that keep that record true.
//!
//! Everything here is pure: callers pass a [`Codebase`](sealmap_model::Codebase)
//! (or two), the lock text and the topic texts; nothing reads the file system
//! or runs `git`. The `sealmap` CLI does the IO.
//!
//! # The lock
//!
//! `seals.lock` sits next to the topics it seals (`docs/diagrams/seals.lock`)
//! and is TOML in one canonical form ([`Lock::to_toml`]):
//!
//! ```toml
//! version = 1
//! algorithm = "sm1"
//! generator = "sealmap 0.1.0"
//!
//! [[topic]]
//! id = "COR-04"
//! file = "corpus/04-seals.md"
//! topic_hash = "blake3-16:4c3d…"
//! reviewer = "zai:glm-5.3"
//! model = "claude:claude-sonnet-5"
//! date = "2026-10-05"
//! symbols = [
//!   { id = "sym:cargo sealmap_corpus . seal/sign/sign().", sig = "blake3-16:…", body = "blake3-16:…" },
//! ]
//! ```
//!
//! `file` is relative to the lock's directory; `reviewer` and `model` are
//! `host:model` routes (see [`route`](crate::route)); [`sign`] refuses a pair
//! that shares a model family or whose family is unknown. Each sealed topic
//! also carries one static pointer line in its front matter, `sealed: seals.lock`, so a topic is self-describing and
//! re-seals never rewrite prose: [`topic_hash`] skips that line.
//!
//! # The checks
//!
//! | Condition | [`Class`] |
//! |---|---|
//! | id resolves, `sig` and `body` match | [`Holds`](Class::Holds) |
//! | `sig` same, `body` differs | [`Behaviour`](Class::Behaviour) |
//! | `sig` differs | [`Contract`](Class::Contract) |
//! | id gone; candidates with the same `body` listed | [`Absent`](Class::Absent) |
//! | a file that may hold the id fails to parse | [`Unparsable`](Class::Unparsable) (fail closed) |
//! | `sym:` cited but not sealed | [`UnsealedCitation`](Class::UnsealedCitation) |
//! | a lock entry with no file, or a sealed symbol no longer cited | [`Orphan`](Class::Orphan) |
//! | `topic_hash` differs | [`ProseEdited`](Class::ProseEdited) |
//! | pointer and lock disagree, or the lock is not canonical | [`LockFault`](Class::LockFault) |
//!
//! [`seal_check`] classifies chosen lock entries; [`verify`] is the corpus
//! gate; [`stale`] lists changed sealed symbols, against the lock or an
//! older model; [`resolve`] answers where one id is now; [`sign`] writes a
//! seal.
//!
//! "May hold the id" is decided from the model alone: a language adapter
//! models a file it cannot parse as a lone module symbol tagged
//! [`PARSE_ERROR_TAG`](sealmap_model::PARSE_ERROR_TAG). A sealed id that is
//! that module, or is absent while one of its ancestor modules is, is
//! unparsable rather than absent.

mod check;
mod lock;
mod sign;
mod topic;

pub use check::{Class, Finding, Resolution, SealReport, StaleSymbol, Topics, resolve, seal_check, stale, verify};
pub use lock::{ALGORITHM, GENERATOR, LOCK_FILE, LOCK_VERSION, Lock, LockError, SymbolSeal, TopicSeal};
pub use sign::{SignError, Signature, sign};
pub use topic::{Citation, POINTER_KEY, citations, pointers, topic_hash, topic_id, with_pointer};
