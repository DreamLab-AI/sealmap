//! Portable per-corpus, per-activity route records.
//!
//! This reader records a confirmed route for a named activity; it neither
//! measures a de-escalation nor executes a model. An absent record selects the
//! caller's default route. No Agentbox manifest is needed.
//!
//! ```json
//! { "version": 1,
//!   "corpora": { "project-docs": { "review-security": "gateway:google/gemini-pro" } } }
//! ```
//!
//! [`Tiers::parse`] accepts arbitrary activity identifiers. Applications with a
//! fixed vocabulary should use [`Tiers::parse_with_activities`] so misspellings
//! fail rather than silently fall back. [`Tiers::parse_agentbox`] is an explicit
//! adapter for that consumer's four activities and narrower route grammar.
//!
//! Schema version, unknown object fields and duplicate keys remain strict under
//! every policy. An absent file means no records; malformed present data never
//! becomes an empty configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::route::{Route, RouteError};

/// Why the tiers file or a start route was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TierError {
    /// The file or activity policy is not valid version-1 route data.
    #[error(".tiers.json is malformed: {0}")]
    Malformed(String),
    /// A route (recorded or default) is outside the selected grammar.
    #[error("{0}")]
    Route(#[from] RouteError),
}

/// Agentbox ADR-2130 activities; used only by the explicit Agentbox adapter.
pub const AGENTBOX_ACTIVITIES: [&str; 4] = ["diagram-polish", "diagram-narrative", "diagram-synthesis", "seal-review"];

/// Compatibility alias for [`AGENTBOX_ACTIVITIES`]. This list does not constrain
/// the generic [`Tiers::parse`] API.
pub const ACTIVITIES: [&str; 4] = AGENTBOX_ACTIVITIES;

/// Refuse repeated JSON object keys instead of keeping the last value.
#[derive(Debug)]
struct Unique<V>(BTreeMap<String, V>);

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Unique<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Walk<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for Walk<V> {
            type Value = Unique<V>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Self::Value, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((k, v)) = m.next_entry::<String, V>()? {
                    if out.contains_key(&k) {
                        return Err(de::Error::custom(format!("duplicate key `{k}`")));
                    }
                    out.insert(k, v);
                }
                Ok(Unique(out))
            }
        }
        d.deserialize_map(Walk(std::marker::PhantomData))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    corpora: Unique<Unique<String>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum RoutePolicy {
    #[default]
    Portable,
    Agentbox,
}

impl RoutePolicy {
    fn parse(self, text: &str) -> Result<Route, RouteError> {
        match self {
            Self::Portable => Route::parse(text),
            Self::Agentbox => Route::parse_agentbox(text),
        }
    }
}

fn activity_name(name: &str) -> Result<(), TierError> {
    if !name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(TierError::Malformed(format!("invalid activity `{name}` (expected an ASCII identifier)")));
    }
    Ok(())
}

fn corpus_name(name: &str) -> Result<(), TierError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(TierError::Malformed("corpus name must be non-empty and contain no control characters".into()));
    }
    Ok(())
}

/// Confirmed routes and the policy used to read them. Lookup applies the same
/// activity/route policy, including when no record exists and a default is used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tiers {
    corpora: BTreeMap<(String, String), String>,
    allowed_activities: Option<BTreeSet<String>>,
    route_policy: RoutePolicy,
}

impl Tiers {
    /// Parse portable version-1 route records. Activity names start with an
    /// ASCII letter and then use letters/digits, `.`, `_` or `-`.
    ///
    /// ```
    /// use sealmap_corpus::tiers::Tiers;
    /// let t = Tiers::parse(r#"{"version":1,"corpora":{"docs":{"review-security":"google:gemini-pro"}}}"#)?;
    /// assert_eq!(t.start_route("docs", "review-security", "local:writer")?, "google:gemini-pro");
    /// assert_eq!(t.start_route("docs", "summary", "local:writer")?, "local:writer");
    /// # Ok::<(), sealmap_corpus::tiers::TierError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, TierError> {
        Self::read(Some(text), None, RoutePolicy::Portable)
    }

    /// Parse records using a caller-supplied activity vocabulary. Unknown
    /// activity names fail both on input and during [`Self::start_route`].
    /// An empty vocabulary admits no activities.
    pub fn parse_with_activities(text: &str, activities: &[&str]) -> Result<Self, TierError> {
        Self::read(Some(text), Some(activities), RoutePolicy::Portable)
    }

    /// Apply Agentbox ADR-2130's four activities and provider/model grammar.
    pub fn parse_agentbox(text: &str) -> Result<Self, TierError> {
        Self::read(Some(text), Some(&AGENTBOX_ACTIVITIES), RoutePolicy::Agentbox)
    }

    /// Read portable records if present. An empty/malformed file is an error.
    pub fn from_file_text(text: Option<&str>) -> Result<Self, TierError> {
        Self::read(text, None, RoutePolicy::Portable)
    }

    /// Read optional records while retaining a caller's activity vocabulary,
    /// even when no file exists and lookups use their supplied defaults.
    pub fn from_file_text_with_activities(text: Option<&str>, activities: &[&str]) -> Result<Self, TierError> {
        Self::read(text, Some(activities), RoutePolicy::Portable)
    }

    /// Read optional records under the explicit Agentbox policy.
    pub fn from_file_text_agentbox(text: Option<&str>) -> Result<Self, TierError> {
        Self::read(text, Some(&AGENTBOX_ACTIVITIES), RoutePolicy::Agentbox)
    }

    fn read(text: Option<&str>, activities: Option<&[&str]>, route_policy: RoutePolicy) -> Result<Self, TierError> {
        let allowed_activities = activities
            .map(|names| {
                names
                    .iter()
                    .map(|name| {
                        activity_name(name)?;
                        Ok((*name).to_owned())
                    })
                    .collect::<Result<BTreeSet<_>, TierError>>()
            })
            .transpose()?;
        let mut tiers = Self { corpora: BTreeMap::new(), allowed_activities, route_policy };
        let Some(text) = text else { return Ok(tiers) };
        let file: File = serde_json::from_str(text).map_err(|e| TierError::Malformed(e.to_string()))?;
        if file.version != 1 {
            return Err(TierError::Malformed(format!("version {} is not supported (expected 1)", file.version)));
        }
        for (name, activities) in file.corpora.0 {
            corpus_name(&name)?;
            for (activity, route) in activities.0 {
                tiers.check_activity(&activity)?;
                route_policy
                    .parse(&route)
                    .map_err(|e| TierError::Malformed(format!("corpus `{name}`, activity `{activity}`: {e}")))?;
                tiers.corpora.insert((name.clone(), activity), route.trim().to_owned());
            }
        }
        Ok(tiers)
    }

    fn check_activity(&self, activity: &str) -> Result<(), TierError> {
        activity_name(activity)?;
        if let Some(allowed) = &self.allowed_activities {
            if !allowed.contains(activity) {
                return Err(TierError::Malformed(format!("unknown activity `{activity}` under selected policy")));
            }
        }
        Ok(())
    }

    /// The recorded route, else the caller's default. Both lookup keys and the
    /// default are checked against the policy selected when reading the file.
    /// This function neither executes a route nor attests its model identity.
    pub fn start_route(&self, corpus: &str, activity: &str, default: &str) -> Result<String, TierError> {
        corpus_name(corpus)?;
        self.check_activity(activity)?;
        self.route_policy.parse(default)?;
        let key = (corpus.to_owned(), activity.to_owned());
        Ok(self.corpora.get(&key).cloned().unwrap_or_else(|| default.trim().to_owned()))
    }
}
