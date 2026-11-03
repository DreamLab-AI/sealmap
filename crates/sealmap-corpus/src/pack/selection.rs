//! Conservative review selection. Symbol changes remain a diagnostic, never
//! a reason to skip a topic whose cited file or build context changed.

use std::collections::BTreeSet;

use sealmap_model::{Codebase, SourcePath};

use crate::seal::{Topics, citations, topic_id};

/// Select topics affected by changed files, including changes anywhere in a
/// cited symbol's file, declared `sources`, and global build/configuration
/// inputs. Missing, malformed or external dependencies are treated as unknown:
/// the topic is selected even if no local file change is known.
///
/// `changed` is relative to the same root as the models. The IO caller must
/// include working-tree and untracked changes, supply `available` as the paths
/// whose presence and repository ownership are known, and separately select
/// changed topic prose. This is a conservative review queue, not a semantic verdict;
/// it cannot discover dependencies absent from both the corpus and model.
///
/// The supported `sources` front matter is a YAML block list or a simple flow
/// list of paths, with optional single/double quotes. Unsupported syntax is
/// unknown rather than silently absent. Configuration/lock/build changes flag
/// every topic because they can alter the consumed implementation or features.
pub fn changed_topics_by_files(
    topics: &Topics,
    changed: &BTreeSet<SourcePath>,
    available: &BTreeSet<SourcePath>,
    before: &Codebase,
    after: &Codebase,
) -> Vec<String> {
    let global = changed.iter().any(is_global_input);
    topics
        .values()
        .filter_map(|text| {
            let id = topic_id(text)?;
            if global {
                return Some(id);
            }
            let mut sources = BTreeSet::new();
            let mut unknown = false;
            match declared_sources(text) {
                Ok(paths) => {
                    for path in paths {
                        if !available.iter().any(|p| p == &path || beneath(p, &path)) {
                            unknown = true;
                        }
                        sources.insert(path);
                    }
                }
                Err(()) => unknown = true,
            }
            for citation in citations(text) {
                let Ok(symbol) = citation.id else {
                    unknown = true;
                    continue;
                };
                let mut found = false;
                for model in [before, after] {
                    if let Some(s) = model.symbol(&symbol) {
                        if !available.contains(&s.file) {
                            unknown = true;
                        }
                        sources.insert(s.file.clone());
                        found = true;
                    }
                }
                if !found {
                    unknown = true;
                }
            }
            (unknown
                || sources.is_empty()
                || sources.iter().any(|p| changed.iter().any(|c| c == p || beneath(p, c) || beneath(c, p))))
            .then_some(id)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn beneath(path: &SourcePath, parent: &SourcePath) -> bool {
    path.as_str().strip_prefix(parent.as_str()).is_some_and(|rest| rest.starts_with('/'))
}

fn is_global_input(path: &SourcePath) -> bool {
    let name = path.file_name();
    matches!(name, "Cargo.toml" | "Cargo.lock" | "build.rs" | ".gitignore" | ".ignore" | "Makefile" | "Dockerfile")
        || name.starts_with("rust-toolchain")
        || name.ends_with(".lock")
        || path.as_str().split('/').any(|part| matches!(part, ".cargo" | ".github"))
        || matches!(path.extension(), Some("toml" | "json" | "yaml" | "yml" | "nix" | "env"))
}

fn declared_sources(text: &str) -> Result<Vec<SourcePath>, ()> {
    let normal = text.replace("\r\n", "\n");
    let Some(rest) = normal.strip_prefix("---\n") else { return Ok(Vec::new()) };
    let Some((front, _)) = rest.split_once("\n---") else { return Err(()) };
    let mut lines = front.lines().peekable();
    let mut result = None;
    while let Some(line) = lines.next() {
        let Some((key, value)) = line.split_once(':') else { continue };
        if key.trim() != "sources" {
            if key.trim().trim_matches(['\'', '"']) == "sources" {
                return Err(());
            }
            continue;
        }
        if result.is_some() {
            return Err(());
        }
        let value = value.trim();
        let mut paths = Vec::new();
        if value.is_empty() {
            while let Some(next) = lines.peek() {
                if !next.starts_with(char::is_whitespace) && !next.starts_with('-') {
                    break;
                }
                let item = lines.next().ok_or(())?.trim().strip_prefix("- ").ok_or(())?;
                paths.push(source_path(item)?);
            }
        } else {
            let list = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')).ok_or(())?;
            if !list.trim().is_empty() {
                for item in list.split(',') {
                    paths.push(source_path(item.trim())?);
                }
            }
        }
        result = Some(paths);
    }
    Ok(result.unwrap_or_default())
}

fn source_path(value: &str) -> Result<SourcePath, ()> {
    let value = if value.starts_with('"') {
        // JSON quoted strings are a safe subset of YAML double-quoted scalars.
        serde_json::from_str::<String>(value).map_err(|_| ())?
    } else if value.starts_with('\'') {
        let inside = value.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).ok_or(())?;
        if inside.replace("''", "").contains('\'') {
            return Err(());
        }
        inside.replace("''", "'")
    } else {
        value.to_owned()
    };
    if value.is_empty() || value.contains(['*', '?', '[', ']', '#', '&', '!']) {
        return Err(());
    }
    SourcePath::new(value).map_err(|_| ())
}
