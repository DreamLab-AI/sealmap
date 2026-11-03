# Changelog

All crates in this workspace share one version.

## 0.4.0 — 2026-11-03

### Added
- `sealmap_corpus::review`: typed adjacent evidence receipts for immutable review
  exports, with explicit inconclusive outcomes, consumed-version records and
  independent source/test/merge/publication/adoption/deployment attestations.
- `sealmap review init/check/summary`: initialise dispositions, validate contracts
  and SHA-256 artefacts within an evidence root, and report the unchanged finding
  denominator. Commands recorded in evidence are never executed.
- Conservative `pack::changed_topics_by_files` selection, including missing or
  unknown coverage, source/configuration changes and submodule path changes.
- Portable route and activity declarations: named providers, namespaced model
  ids, application-defined activities and explicit family catalogues. Fixed
  activity vocabularies can use `Tiers::parse_with_activities`.

### Changed
- Pack wire format is version 2: the header includes a digest of all normalised
  extraction inputs, with optional deterministic CLI repository/build-context
  provenance. `pack --diff` uses file-level selection, not symbol hashes.
- `pack -o` requires a fresh file; sharded output requires a fresh directory.
  An empty selection with `-o` exits 1 and leaves old output untouched. Existing
  files, directories and symlinks are never silently replaced or mixed with new
  shards. Use a new output directory per run.
- Documentation and regression expectations incorporate the estate audit's
  distinction between source, package, consumer and live-runtime evidence.
  Seals remain experimental and model agreement remains a ranking signal.
- `Route::parse` and `Tiers::parse` now use the portable policy. Consumers that
  require the 0.3 Agentbox vocabulary must select `parse_agentbox` explicitly.
  `Family` is `Clone` rather than `Copy` to support caller-defined families.

### Fixed
- Model-family recognition now respects name/version boundaries: foreign names
  such as `gpt-j` require explicit declarations and cannot earn false
  cross-family review credit through a shared prefix.
- Uncited changes in a cited file, dirty/untracked inputs and unknown source
  coverage can no longer disappear behind unchanged symbol hashes.
- Multi-repository packs identify each source baseline; untracked extraction
  inputs and lock/build context affect provenance.
- `ContentHash` deserialisation validates and normalises its representation;
  malformed input can no longer violate the invariant used by `short()`.
- Seal lock/topic IO, dense output and pack output refuse checkout symlink
  paths. Dense and seal writes stage content before replacing destinations;
  malformed seal-lock diagnostics no longer echo input values. Temporary Git
  exports use private, uniquely created directories and are removed on failure.
- Fingerprint parsing rejects signed byte pairs and requires ASCII hexadecimal
  digits. Model-family checks recognise known model identities, reject
  conflicting declarations and do not infer a
  family from a provider label alone.

### Limits
- Receipt checks establish contract consistency and artefact integrity;
  reviewers still judge the observations and decision authority. Delivery
  counts are attestations, not completion percentages.
- File-based selection deliberately keeps uncertain topics. Symbol staleness
  and seals remain experimental; packs have not been shown to improve review
  quality over full source when that source fits in context.
- Local path checks do not provide a sandbox against concurrent hostile
  filesystem replacement. A failed pack write can leave newly created partial
  output, so consumers must check the command's exit status.

## 0.3.0 — 2026-10-31

### Added
- `seal::sign` (and so `sealmap seal sign`) refuses a seal review whose
  reviewer shares a model family with the author, or whose family is unknown,
  with `SignError::Family` and exit status 1; nothing is written (agentbox
  ADR-2130). `--reviewer` must be exactly one `host:model` (the model that gave the verdict;
  a multi-rung route is refused with exit 1 and nothing is written); `--model`
  is `host:model` or `a -> b` when an escalation ran, and every author rung is
  checked against the reviewer. The family of a `loom` route comes from its
  model id and is not verified against what the Loom serves.
- `route` module: the manifest route grammar (`host:model [-> host:model]`,
  hosts `claude`, `codex`, `loom`, `zai`), `Family`, and `cross_family`.
- `tiers` module: a strict reader for `docs/diagrams/.tiers.json`
  (confirmed de-escalations, per corpus and per activity), with
  `start_route(corpus, activity, default)` falling back to the route default
  when there is no record. A malformed file, a duplicated key or an
  activity other than `diagram-polish`, `diagram-narrative`,
  `diagram-synthesis` or `seal-review` is an error. It reads only; the measured descent is not part of sealmap.

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
