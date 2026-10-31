//! The `docs/diagrams/.tiers.json` reader (agentbox ADR-2130).
//!
//! The file records, per corpus and per activity, a confirmed de-escalation:
//! the route a measured descent settled on. Activities have different default
//! routes (`diagram-polish` is T1, `diagram-synthesis` is T3), so a record
//! must say which activity it lowered. It is sticky per corpus. An activity
//! with no record starts from the route default. This module only reads the
//! record; it measures nothing and writes nothing.
//!
//! ```json
//! { "version": 1,
//!   "corpora": { "sealmap": { "diagram-polish": "claude:claude-sonnet-5-5" } } }
//! ```
//!
//! The format (a `corpora` -> activity -> route map) is this crate's reading
//! of ADR-2130, which fixes the file's name and its stickiness but not its
//! layout.
//!
//! The schema is version 1, strict: unknown keys, a duplicated key, an
//! activity outside [`ACTIVITIES`], a wrong version or a route outside the
//! manifest grammar are errors. An absent file means "no
//! records"; a present but malformed one is never read as empty.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::route::{Route, RouteError};

/// Why the tiers file or a start route was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TierError {
    /// The file is not valid version-1 `.tiers.json`.
    #[error(".tiers.json is malformed: {0}")]
    Malformed(String),
    /// A route (recorded or default) is outside the route grammar.
    #[error("{0}")]
    Route(#[from] RouteError),
}

/// The activities ADR-2130 routes. A record for any other name is refused.
pub const ACTIVITIES: [&str; 4] = ["diagram-polish", "diagram-narrative", "diagram-synthesis", "seal-review"];

/// A JSON object read into a map that refuses a repeated key (serde's
/// `BTreeMap` keeps the last one silently).
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

/// The confirmed de-escalations, per corpus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tiers {
    corpora: BTreeMap<(String, String), String>,
}

impl Tiers {
    /// Parse `.tiers.json` text.
    ///
    /// ```
    /// use sealmap_corpus::tiers::Tiers;
    ///
    /// let t = Tiers::parse(r#"{"version":1,"corpora":{"c":{"diagram-polish":"claude:claude-sonnet-5-5"}}}"#).unwrap();
    /// assert_eq!(t.start_route("c", "diagram-polish", "loom:qwen3.8-27B").unwrap(), "claude:claude-sonnet-5-5");
    /// assert_eq!(t.start_route("c", "diagram-synthesis", "loom:qwen3.8-27B").unwrap(), "loom:qwen3.8-27B");
    /// ```
    pub fn parse(text: &str) -> Result<Self, TierError> {
        let file: File = serde_json::from_str(text).map_err(|e| TierError::Malformed(e.to_string()))?;
        if file.version != 1 {
            return Err(TierError::Malformed(format!("version {} is not supported (expected 1)", file.version)));
        }
        let mut corpora = BTreeMap::new();
        for (name, activities) in file.corpora.0 {
            for (activity, route) in activities.0 {
                if !ACTIVITIES.contains(&activity.as_str()) {
                    return Err(TierError::Malformed(format!(
                        "corpus `{name}`: unknown activity `{activity}` (expected one of {})",
                        ACTIVITIES.join(", ")
                    )));
                }
                Route::parse(&route)
                    .map_err(|e| TierError::Malformed(format!("corpus `{name}`, activity `{activity}`: {e}")))?;
                corpora.insert((name.clone(), activity), route.trim().to_owned());
            }
        }
        Ok(Self { corpora })
    }

    /// The tiers from the file's text if the file exists (`Some`), or no
    /// records if it does not (`None`). `Some("")` is malformed, not absent.
    pub fn from_file_text(text: Option<&str>) -> Result<Self, TierError> {
        text.map_or_else(|| Ok(Self::default()), Self::parse)
    }

    /// The route to start `activity` from on `corpus`: its confirmed route,
    /// else `default` (the manifest route for the activity). `default` must
    /// parse.
    pub fn start_route(&self, corpus: &str, activity: &str, default: &str) -> Result<String, TierError> {
        Route::parse(default)?;
        let key = (corpus.to_owned(), activity.to_owned());
        Ok(self.corpora.get(&key).cloned().unwrap_or_else(|| default.trim().to_owned()))
    }
}
