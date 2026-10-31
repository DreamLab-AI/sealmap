//! Model routes and model families (agentbox ADR-2130).
//!
//! A route is the manifest's `[model_routing.routes]` grammar:
//! `host:model` or `host:model -> host:model`, where `host` is one of
//! `claude`, `codex`, `loom` or `zai`. The grammar matches agentbox's
//! `services/agentbox-manifest/src/routing.rs` (one start, at most one rung).
//!
//! # Families
//!
//! A seal review must come from a different model family than the author
//! ([`cross_family`]). The family of a rung is decided like this:
//!
//! | Model id starts with | Family |
//! |---|---|
//! | `claude` | [`Anthropic`](Family::Anthropic) |
//! | `gpt`, `o1`..`o9`, `codex` | [`OpenAi`](Family::OpenAi) |
//! | `glm` | [`Zhipu`](Family::Zhipu) |
//! | `qwen` | [`Alibaba`](Family::Alibaba) |
//!
//! Hosts `claude`, `codex` and `zai` fix a family (Anthropic, OpenAI, Zhipu)
//! when the id is not recognised; a recognised id that contradicts its host
//! (`claude:glm-5.3`) is refused. `loom` fixes nothing, because the model
//! behind the Loom is swappable: its family is that of the model id, and an
//! id this table does not know is an unknown family. Unknown families are
//! refused, never passed: the check fails closed.
//!
//! A loom family is read from the route's model id as declared. It is not
//! checked against what the Loom actually serves, so callers should pass the
//! model id the Loom reported.

use std::collections::BTreeSet;
use std::fmt;

/// A model family: who trained the model, hence whose mistakes it shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Family {
    /// Claude models.
    Anthropic,
    /// GPT and Codex models.
    OpenAi,
    /// GLM models (Z.AI).
    Zhipu,
    /// Qwen models.
    Alibaba,
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Family::Anthropic => "anthropic",
            Family::OpenAi => "openai",
            Family::Zhipu => "zhipu",
            Family::Alibaba => "alibaba",
        })
    }
}

/// Why a route was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RouteError {
    /// The text is not `host:model` or `host:model -> host:model`.
    #[error(
        "`{0}` is not a route (expected `host:model` or `host:model -> host:model`, host one of claude, codex, loom, zai)"
    )]
    Grammar(String),
    /// No family is known for this rung, so it cannot be shown to differ.
    #[error("unknown model family for `{rung}`: refusing (the check fails closed)")]
    UnknownFamily {
        /// The rung, `host:model`.
        rung: String,
    },
    /// The model id belongs to a different family than its host serves.
    #[error("`{rung}`: host serves {host} but the model id is {model}")]
    HostModelMismatch {
        /// The rung, `host:model`.
        rung: String,
        /// The family the host fixes.
        host: Family,
        /// The family the model id names.
        model: Family,
    },
    /// Author and reviewer share a family.
    #[error(
        "seal review refused: author `{author}` and reviewer `{reviewer}` are the same model family ({family}); a seal review must come from a different family"
    )]
    SameFamily {
        /// The author route as given.
        author: String,
        /// The reviewer route as given.
        reviewer: String,
        /// The shared family.
        family: Family,
    },
}

/// One `host:model` rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rung {
    /// `claude`, `codex`, `loom` or `zai`.
    pub host: String,
    /// The model id.
    pub model: String,
}

/// A parsed route: a start rung and at most one escalation rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The start rung, then the escalation rung if any.
    pub rungs: Vec<Rung>,
}

const HOSTS: [&str; 4] = ["claude", "codex", "loom", "zai"];

fn rung(text: &str) -> Option<Rung> {
    let (host, model) = text.trim().split_once(':')?;
    let ok = !model.is_empty() && model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    (HOSTS.contains(&host) && ok).then(|| Rung { host: host.into(), model: model.into() })
}

fn by_model(model: &str) -> Option<Family> {
    let m = model.to_ascii_lowercase();
    let o_series = m.len() > 1 && m.starts_with('o') && m.as_bytes()[1].is_ascii_digit();
    if m.starts_with("claude") {
        Some(Family::Anthropic)
    } else if m.starts_with("gpt") || m.starts_with("codex") || o_series {
        Some(Family::OpenAi)
    } else if m.starts_with("glm") {
        Some(Family::Zhipu)
    } else if m.starts_with("qwen") {
        Some(Family::Alibaba)
    } else {
        None
    }
}

impl Rung {
    /// The family of this rung, or why it cannot be decided.
    pub fn family(&self) -> Result<Family, RouteError> {
        let label = format!("{}:{}", self.host, self.model);
        let host = match self.host.as_str() {
            "claude" => Some(Family::Anthropic),
            "codex" => Some(Family::OpenAi),
            "zai" => Some(Family::Zhipu),
            _ => None,
        };
        match (host, by_model(&self.model)) {
            (Some(h), Some(m)) if h != m => Err(RouteError::HostModelMismatch { rung: label, host: h, model: m }),
            (Some(f), _) | (None, Some(f)) => Ok(f),
            (None, None) => Err(RouteError::UnknownFamily { rung: label }),
        }
    }
}

impl Route {
    /// Parse the manifest route grammar.
    ///
    /// ```
    /// use sealmap_corpus::route::Route;
    ///
    /// let r = Route::parse("zai:glm-5.3 -> claude:claude-fable-5-1").unwrap();
    /// assert_eq!(r.rungs.len(), 2);
    /// assert!(Route::parse("gemini:flash").is_err());
    /// ```
    pub fn parse(text: &str) -> Result<Self, RouteError> {
        let bad = || RouteError::Grammar(text.to_owned());
        let parts: Vec<&str> = text.split("->").collect();
        if parts.len() > 2 {
            return Err(bad());
        }
        let rungs = parts.into_iter().map(|p| rung(p).ok_or_else(bad)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { rungs })
    }

    /// The families of every rung. Fails if any rung's family is unknown.
    pub fn families(&self) -> Result<BTreeSet<Family>, RouteError> {
        self.rungs.iter().map(Rung::family).collect()
    }
}

/// Check that `reviewer` shares no family with `author`. Both are routes
/// (a lone `host:model` is a route); every rung of each counts, so a
/// reviewer whose fallback rung is the author's family is refused too.
/// Fails closed: a malformed route or an unknown family is an error.
///
/// ```
/// use sealmap_corpus::route::cross_family;
///
/// assert!(cross_family("claude:claude-sonnet-5-5", "zai:glm-5.3").is_ok());
/// assert!(cross_family("claude:claude-sonnet-5-5", "claude:claude-fable-5-1").is_err());
/// ```
pub fn cross_family(author: &str, reviewer: &str) -> Result<(), RouteError> {
    let a = Route::parse(author)?.families()?;
    let r = Route::parse(reviewer)?.families()?;
    match a.intersection(&r).next() {
        None => Ok(()),
        Some(&family) => {
            Err(RouteError::SameFamily { author: author.trim().into(), reviewer: reviewer.trim().into(), family })
        }
    }
}
