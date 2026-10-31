# Changelog

All crates in this workspace share one version.

## Unreleased

### Added
- `seal::sign` (and so `sealmap seal sign`) refuses a seal review whose
  reviewer shares a model family with the author, or whose family is unknown,
  with `SignError::Family` and exit status 1; nothing is written (agentbox
  ADR-2130). `--reviewer` and `--model` are now `host:model` routes.
- `route` module: the manifest route grammar (`host:model [-> host:model]`,
  hosts `claude`, `codex`, `loom`, `zai`), `Family`, and `cross_family`.
- `tiers` module: a strict reader for `docs/diagrams/.tiers.json`
  (confirmed de-escalations, per corpus and per activity), with
  `start_route(corpus, activity, default)` falling back to the route default
  when there is no record. A malformed file is an
  error. It reads only; the measured descent is not part of sealmap.

### Changed
- Adds `SignError::Family` to an exhaustive enum, so the next release is
  0.3.0, not 0.2.x.
- Seals signed with opaque reviewer or model strings are now refused. Existing
  locks still verify: only the write path applies the policy.

### Docs
- The README positions sealmap as a deterministic code lens for Rust
  (extraction, generated views, `dense`, `pack`) and adds "What the evidence
  says", with a table linking every experiment's results. The seal commands
  (`seal sign`, `verify`, `seal-check`, `stale`, `resolve`) stay documented,
  marked experimental and not recommended for corpus upkeep: every staleness
  rule finer than per-file lost real changes (recall 0.76 to 0.38) for at most
  about 2× fewer flags. The claim that seals cut upkeep is removed.
- `docs/DESIGN.md` opens with a dated status after the evidence programme:
  the code-lens surface is kept, seal and staleness work is frozen, and the
  TypeScript adapter is dropped.
- The diagram topics whose prose contradicted that status are updated and
  re-stamped.

## 0.2.1 — 2026-10-06

### Fixed
- `write` (and so `sealmap generate`) followed symbolic links in the output
  directory and could write outside it. It now refuses, before changing
  anything, any path that runs through a link below the output directory.
- A binding made by an `if let` or `while let` condition stayed in scope after
  the statement (and in its `else` branches), so a later call on a shadowed
  outer name was drawn as an Exact call on the condition's type.
- `#[cfg]` twins (two definitions with one id) kept only the first
  definition's flow, so the second twin's calls were missing from its sequence
  and from the call relations. Twins whose flows differ now become the arms of
  one branch, each labelled `cfg twin at <file>:<line>`.
- An impl whose self type is not a path (a tuple, slice, array, function
  pointer or trait object) was skipped with all its methods and calls. Its
  methods are now anchored like those of any type the codebase does not
  define, e.g. ``sym:cargo app . impl#[`(u8, u8)`][T]go().``.
- `generate --no-model` left a `_model.json` from an earlier run in place,
  and `generate --check` passed with it. A sealmap model the generation leaves
  out is now orphaned: reported by `--check`, deleted by `generate`. A
  `_model.json` that is not a sealmap model is still left alone.
- Crates with the same name (two packages, or test, bench and example
  targets with one file name in two packages) became one crate, and their
  symbols merged. A crate name held by more than one directory is now
  qualified with the package directory (`sym:cargo a/core . run().`, `./core`
  at the root); a path naming such a crate resolves through the caller's
  manifest (`path` dependencies, also via `[workspace.dependencies]`), or stays
  external when it is still ambiguous. Two `--repo` entries with one name, or
  one nested in the other's prefix, are refused instead of overwriting each
  other's files.

## 0.2.0 — 2026-10-06

### Added
- `sym:` symbol ids (SCIP-descriptor style): readable, never file-path based,
  injective; canonical text round-trips exactly.
- Per-symbol `sig_hash` and `body_hash` (BLAKE3-16, algorithm `sm1`), blind to
  formatting, comments and moves; schema v2 carries both on every symbol.
- The seal surface in `sealmap-corpus` and the CLI: `seals.lock`, `topic_hash`,
  `resolve`, `seal-check`, `stale [--since REV]`, `verify` and `seal sign`.
- `sealmap-dense`: skeletons with `L<start>-<end>` spans, indented call trees, a
  short-name index and byte-budgeted slices that refuse rather than truncate.
  Measured at 0.17-0.29× source size (0.32-0.46× with the index).
- `sealmap pack`: a deterministic review pack from pegged topics, by id or by
  `--diff REV`, with dense slices and bounded source windows; over budget it
  refuses and names each topic's size.
- `generate --check`: compare a directory with a fresh generation.

### Changed
- Renamed from `codemaid-*`; licence is now `MIT OR Apache-2.0`.
- `sealmap-frontend` is renamed `sealmap-extract`; `sealmap-frontend` 0.1.1 is
  a deprecated shim re-exporting it.
- Generated output goes to a gitignored `.sealmap/` and is no longer committed.
- Mermaid diagram ids are derived from `sym:` ids and cannot collide.

### Fixed
- Stack overflow on deep generic types; `.gitignore` ignored by the loader;
  exponential glob-import resolution.
- Std and third-party receivers bound to same-named internal methods
  (`Path::parent()` drawn as `SymbolId::parent`).
- Single-capital type names treated as generic parameters; declared
  multi-letter generics not treated as generics.
- `Self::f(..)` calls dropped; calls in match guards not walked; method calls
  on struct literals lost; guessed edges unstable across unrelated crates.

### Minimum Rust version
- 1.85, checked in CI. `Cargo.lock` pins `ignore` 0.4.29: 0.4.30 needs Rust
  1.88 without declaring it. Outside this workspace on 1.85, run
  `cargo update -p ignore --precise 0.4.29`.

## 0.1.0 — 2026-10-05

First release, as `sealmap-*` (formerly `codemaid`).
