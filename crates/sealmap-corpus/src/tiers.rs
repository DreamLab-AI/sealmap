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
//! The schema is version 1, strict: unknown keys, a wrong version or a route
//! outside the manifest grammar are errors. An absent file means "no
//! records"; a present but malformed one is never read as empty.

use std::collections::BTreeMap;

use serde::Deserialize;

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    corpora: BTreeMap<String, BTreeMap<String, String>>,
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
        for (name, activities) in file.corpora {
            for (activity, route) in activities {
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
