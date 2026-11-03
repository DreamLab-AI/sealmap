//! Portable model-route declarations and fail-closed family checks.
//!
//! A route contains one `provider:model` or two separated by `->`. Providers
//! are identifiers, not network endpoints; parsing never executes a model or
//! authorises network access. Model identifiers may contain namespaces and tags
//! (`gateway:google/gemini-pro`, `local:team/model:latest`).
//!
//! [`cross_family`] recognises model names from known families. It never derives
//! a family solely from a provider name. Use [`FamilyCatalogue`] for exact aliases
//! or another family's models; declarations are supplied by the caller and are
//! **not** attestations of what a remote endpoint actually served. Record the
//! actual responding model in review evidence. Unknown families fail closed.
//! Recognition uses bounded model-name stems and versions, not arbitrary shared
//! prefixes: for example, `gpt-j-6b` needs a catalogue declaration. This is a
//! conservative naming convention, not an exhaustive registry of model releases.
//!
//! The default API is independent of an orchestrator. [`Route::parse_agentbox`]
//! preserves Agentbox's narrower provider/model grammar as an explicit adapter
//! policy. It does not change the generic grammar or the family-checking rules.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The declared training family of a model, distinct from its serving provider.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    /// Gemini models.
    Google,
    /// Another caller-declared family. Catalogue insertion validates and
    /// canonicalises the label; prefer [`Family::named`].
    Custom(String),
}

impl Family {
    /// Parse a family label. Known aliases canonicalise to the built-in family
    /// so `claude` and `anthropic` cannot receive independence credit.
    ///
    /// Custom labels use ASCII letters/digits, `.`, `_` or `-`, starting with
    /// a letter, and are normalised to lower case. They declare an identity;
    /// the caller remains responsible for its accuracy.
    pub fn named(label: &str) -> Result<Self, RouteError> {
        if !identifier(label) {
            return Err(RouteError::FamilyDeclaration(format!("invalid family label `{label}`")));
        }
        Ok(match label.to_ascii_lowercase().as_str() {
            "anthropic" | "claude" => Self::Anthropic,
            "openai" | "gpt" | "codex" => Self::OpenAi,
            "zhipu" | "zai" | "glm" => Self::Zhipu,
            "alibaba" | "qwen" => Self::Alibaba,
            "google" | "gemini" => Self::Google,
            _ => Self::Custom(label.to_ascii_lowercase()),
        })
    }

    fn canonical(self) -> Result<Self, RouteError> {
        match self {
            Self::Custom(name) => Self::named(&name),
            other => Ok(other),
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Zhipu => "zhipu",
            Self::Alibaba => "alibaba",
            Self::Google => "google",
            Self::Custom(label) => label,
        })
    }
}

/// Why a route or declared family was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RouteError {
    /// The text is not one or two portable `provider:model` rungs.
    #[error("`{0}` is not a route (expected `provider:model` or `provider:model -> provider:model`)")]
    Grammar(String),
    /// The explicit Agentbox adapter policy rejected a portable route.
    #[error("`{0}` is outside the Agentbox route policy (providers: claude, codex, loom, zai; simple model ids)")]
    AgentboxPolicy(String),
    /// No family is known for this rung, so it cannot be shown to differ.
    #[error("unknown model family for `{rung}`: refusing (the check fails closed)")]
    UnknownFamily {
        /// The rung, `provider:model`.
        rung: String,
    },
    /// The model id or declared alias contradicts a known family-specific provider.
    #[error("`{rung}`: provider declares {host} but the model declares {model}")]
    HostModelMismatch {
        /// The rung, `provider:model`.
        rung: String,
        /// The family the provider label declares.
        host: Family,
        /// The model's recognised or explicitly declared family.
        model: Family,
    },
    /// A catalogue declaration is invalid or contradicts an existing identity.
    #[error("invalid model-family declaration: {0}")]
    FamilyDeclaration(String),
    /// A reviewer must name the one model that gave the verdict, not a fallback route.
    #[error("reviewer `{0}` must name exactly one model")]
    ReviewerRoute(String),
    /// Author and reviewer share a family.
    #[error(
        "seal review refused: author `{author}` and reviewer `{reviewer}` are the same model family ({family}); a seal review must come from a different family"
    )]
    SameFamily {
        /// The author route as given.
        author: String,
        /// The reviewer rung as given.
        reviewer: String,
        /// The shared family.
        family: Family,
    },
}

/// One `provider:model` declaration. `host` retains its pre-0.4 field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rung {
    /// Provider identifier, not an address or a claim of model identity.
    pub host: String,
    /// Model identifier, optionally namespaced and/or tagged.
    pub model: String,
}

/// A start rung and at most one escalation rung. This is data, not an executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The start rung, then the escalation rung if any.
    pub rungs: Vec<Rung>,
}

const AGENTBOX_HOSTS: [&str; 4] = ["claude", "codex", "loom", "zai"];

fn identifier(text: &str) -> bool {
    text.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn rung(text: &str) -> Option<Rung> {
    let (host, model) = text.trim().split_once(':')?;
    let model_ok = !model.is_empty()
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b':' | b'@' | b'+'));
    (identifier(host) && model_ok).then(|| Rung { host: host.into(), model: model.into() })
}

fn named_stem(model: &str, stem: &str) -> bool {
    model == stem
        || model.strip_prefix(stem).and_then(|rest| rest.strip_prefix('-')).is_some_and(|rest| !rest.is_empty())
}

fn numbered_model(model: &str, prefix: &str, omni_suffix: bool) -> bool {
    let Some(rest) = model.strip_prefix(prefix) else { return false };
    let version = match rest.split_once('-') {
        Some((version, suffix)) if !suffix.is_empty() => version,
        Some(_) => return false,
        None => rest,
    };
    // GPT-4o is a version token; `gpt-4all` or `o3other` is not. A suffix
    // describing a size, variant or date starts at a hyphen boundary.
    let version = if omni_suffix { version.strip_suffix('o').unwrap_or(version) } else { version };
    version.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn by_model(model: &str) -> Option<Family> {
    // Namespaces and local tags do not change a recognised model's family.
    let m = model.rsplit('/').next()?.split([':', '@']).next()?.to_ascii_lowercase();
    if named_stem(&m, "claude") {
        Some(Family::Anthropic)
    } else if numbered_model(&m, "gpt-", true)
        || named_stem(&m, "gpt-oss")
        || named_stem(&m, "codex")
        || numbered_model(&m, "o", false)
    {
        Some(Family::OpenAi)
    } else if named_stem(&m, "glm") {
        Some(Family::Zhipu)
    } else if named_stem(&m, "qwen") || numbered_model(&m, "qwen", false) {
        Some(Family::Alibaba)
    } else if named_stem(&m, "gemini") {
        Some(Family::Google)
    } else {
        None
    }
}

fn by_provider(host: &str) -> Option<Family> {
    // These labels may contradict a model declaration, but cannot establish an
    // otherwise unknown model's identity. Arbitrary gateways fix no family.
    match host.to_ascii_lowercase().as_str() {
        "claude" | "anthropic" => Some(Family::Anthropic),
        "codex" | "openai" => Some(Family::OpenAi),
        "zai" | "zhipu" => Some(Family::Zhipu),
        "gemini" | "google" => Some(Family::Google),
        _ => None,
    }
}

fn check_provider(rung: &Rung, model: &Family) -> Result<(), RouteError> {
    if let Some(host) = by_provider(&rung.host) {
        if &host != model {
            return Err(RouteError::HostModelMismatch {
                rung: format!("{}:{}", rung.host, rung.model),
                host,
                model: model.clone(),
            });
        }
    }
    Ok(())
}

/// Explicit family declarations for exact aliases or models without recognised
/// names. No provider-wide fallback is inferred or installed.
///
/// Conflicting declarations and attempts to relabel a known model are refused.
/// This catalogue belongs to the calling application; it makes no network calls
/// and does not prove that a server used the declared model.
///
/// ```
/// use sealmap_corpus::route::{Family, FamilyCatalogue, cross_family_with};
/// let mut catalogue = FamilyCatalogue::default();
/// catalogue.declare("local:writer", Family::Anthropic)?;
/// catalogue.declare("gateway:reviewer", Family::Google)?;
/// cross_family_with("local:writer", "gateway:reviewer", &catalogue)?;
/// # Ok::<(), sealmap_corpus::route::RouteError>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyCatalogue {
    declarations: BTreeMap<(String, String), Family>,
}

impl FamilyCatalogue {
    /// Declare the family of exactly one `provider:model` alias. Redeclaring
    /// the same canonical family is idempotent; changing it is an error.
    pub fn declare(&mut self, alias: &str, family: Family) -> Result<(), RouteError> {
        let route = Route::parse(alias)?;
        if route.rungs.len() != 1 {
            return Err(RouteError::FamilyDeclaration("an alias must name exactly one model".into()));
        }
        let r = &route.rungs[0];
        let family = family.canonical()?;
        check_provider(r, &family)?;
        if by_model(&r.model).is_some_and(|known| known != family) {
            return Err(RouteError::FamilyDeclaration(format!("`{alias}` contradicts its recognised model family")));
        }
        let key = (r.host.clone(), r.model.clone());
        if let Some(existing) = self.declarations.get(&key) {
            if existing != &family {
                return Err(RouteError::FamilyDeclaration(format!("`{alias}` already declares {existing}")));
            }
        }
        self.declarations.insert(key, family);
        Ok(())
    }
}

impl Rung {
    /// Resolve a known model name, refusing unknown identities and provider
    /// contradictions. A provider label alone never supplies the family.
    pub fn family(&self) -> Result<Family, RouteError> {
        self.family_with(&FamilyCatalogue::default())
    }

    /// Resolve a model using recognised names and exact catalogue declarations.
    pub fn family_with(&self, catalogue: &FamilyCatalogue) -> Result<Family, RouteError> {
        let label = format!("{}:{}", self.host, self.model);
        if rung(&label).as_ref() != Some(self) {
            return Err(RouteError::Grammar(label));
        }
        let model = catalogue
            .declarations
            .get(&(self.host.clone(), self.model.clone()))
            .cloned()
            .or_else(|| by_model(&self.model))
            .ok_or(RouteError::UnknownFamily { rung: label })?;
        check_provider(self, &model)?;
        Ok(model)
    }
}

impl Route {
    /// Parse the portable grammar, without requiring a known provider or family.
    ///
    /// ```
    /// use sealmap_corpus::route::Route;
    /// let r = Route::parse("local:team/writer:latest -> gateway:google/gemini-pro")?;
    /// assert_eq!(r.rungs.len(), 2);
    /// assert!(r.families().is_err()); // writer needs an explicit family declaration
    /// # Ok::<(), sealmap_corpus::route::RouteError>(())
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

    /// Apply Agentbox's four-provider, simple-model-id grammar explicitly.
    /// This adapter does not execute routes or change family inference.
    pub fn parse_agentbox(text: &str) -> Result<Self, RouteError> {
        let parsed = Self::parse(text)?;
        if parsed.rungs.iter().any(|r| {
            !AGENTBOX_HOSTS.contains(&r.host.as_str())
                || !r.model.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        }) {
            return Err(RouteError::AgentboxPolicy(text.into()));
        }
        Ok(parsed)
    }

    /// The family of every rung. Unknown identities are errors.
    pub fn families(&self) -> Result<BTreeSet<Family>, RouteError> {
        self.families_with(&FamilyCatalogue::default())
    }

    /// The family of every rung, using exact caller-supplied declarations.
    pub fn families_with(&self, catalogue: &FamilyCatalogue) -> Result<BTreeSet<Family>, RouteError> {
        if self.rungs.is_empty() || self.rungs.len() > 2 {
            return Err(RouteError::Grammar("expected one or two model rungs".into()));
        }
        self.rungs.iter().map(|r| r.family_with(catalogue)).collect()
    }
}

/// Require the single model that delivered a review verdict to differ from
/// every authoring rung. Unknown model identities fail closed.
///
/// ```
/// use sealmap_corpus::route::cross_family;
/// assert!(cross_family("gateway:anthropic/claude-sonnet", "google:gemini-pro").is_ok());
/// assert!(cross_family("local:claude-sonnet", "anthropic:claude-opus").is_err());
/// ```
pub fn cross_family(author: &str, reviewer: &str) -> Result<(), RouteError> {
    cross_family_with(author, reviewer, &FamilyCatalogue::default())
}

/// [`cross_family`] with explicit aliases or additional training families.
/// Catalogue declarations describe caller knowledge, not runtime attestation.
pub fn cross_family_with(author: &str, reviewer: &str, catalogue: &FamilyCatalogue) -> Result<(), RouteError> {
    let a = Route::parse(author)?.families_with(catalogue)?;
    let review = Route::parse(reviewer)?;
    if review.rungs.len() != 1 {
        return Err(RouteError::ReviewerRoute(reviewer.trim().into()));
    }
    let r = review.families_with(catalogue)?;
    match a.intersection(&r).next() {
        None => Ok(()),
        Some(family) => Err(RouteError::SameFamily {
            author: author.trim().into(),
            reviewer: reviewer.trim().into(),
            family: family.clone(),
        }),
    }
}
