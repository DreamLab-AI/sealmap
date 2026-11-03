# sealmap

[![crates.io](https://img.shields.io/crates/v/sealmap.svg)](https://crates.io/crates/sealmap)
[![docs.rs](https://img.shields.io/docsrs/sealmap)](https://docs.rs/sealmap)

Standalone Rust code inspection and review-evidence tooling. This crate is the
facade and CLI for [Sealmap](https://github.com/DreamLab-AI/sealmap): source
extraction, generated Mermaid diagrams, compact call trees, bounded review
packs and evidence receipt checks. No model service, account or Agentbox runtime
is required.

The Rust adapter parses without compiling or type-checking. It labels inferred
and external relations, and stable `sym:` ids locate definitions independently
of their files. Generated views help inspect a system; receipt checks establish
artefact integrity and consistency, not the truth of every review claim.

## Install and explore

These commands target 0.4.0. While publication is pending, install from the
repository with `cargo install --path crates/sealmap --locked`.

```sh
cargo install sealmap --version 0.4.0 --locked
sealmap dense . --stats
sealmap generate .
sealmap generate . --check
sealmap model . > .sealmap/model.json
sealmap resolve 'sym:cargo checkout . total().'
```

Generated output defaults to `.sealmap/`; keep it ignored. `dense` writes
`dense.txt` and `_index.txt` beneath `.sealmap/dense/`. Include `--tests` when
test code belongs in the view. The
[repository walkthrough](https://github.com/DreamLab-AI/sealmap#walk-through-a-small-project)
creates an executable fixture for these commands.

## Review packs

An authored topic supplies a front-matter `id`, declared `sources` and optional
`sym:` citations. Pack selected topics with their dense slices and source windows:

```sh
sealmap pack SHOP-01 --budget 200000 -o review-new.txt
sealmap pack --diff main --budget 200000 --shard -o packs-new
sealmap pack API-01 --repo api=../api --repo core=../core \
  --budget 200000 -o combined-new.txt
```

Budgets are bytes. Packs refuse over-budget requests rather than silently
cutting topics; source windows are explicitly bounded and labelled. `--diff`
uses cited files and build context, conservatively keeping unknown coverage.
It cannot combine with `--repo`. Topics that only cite `path:line` are included
as text; `sym:` citations enable code slices.

Format 2 binds normalised extraction inputs and records repository baselines,
dirty/untracked inputs and local build-context hashes. These records do not
establish what a deployed consumer uses. Output files and shard directories
must be fresh. An empty diff selection with `-o` exits 1 and preserves existing
output; callers must respect that status.

## Evidence receipts

Keep the original JSON array of findings unchanged, with a unique string `id`
per entry. Create an adjacent receipt, add actual observations, then validate:

```sh
sealmap review init findings.json --out receipts.json
sealmap review check receipts.json --json
sealmap review summary receipts.json
sealmap review check receipts.json --require-resolved
```

An initial receipt has unverified findings. `--require-resolved` exits 1 until
unverified and inconclusive assessments are resolved. Confirmation needs scoped
product-failure evidence; rejection needs counterevidence. Failed setup is
inconclusive. Explicit accepted risks name their decision authority.

Each observation records the exact revision, configuration, captured artefact
and SHA-256. Executed reproductions include the command and a positive control.
The tool hashes artefacts but never executes recorded commands. Delivery stages
(`source`, `tested`, `merged`, `published`, `adopted`, `deployed`) need independent
evidence and are not a completion percentage.

Artefacts are relative to the receipt directory, or `--evidence-root DIR`.
JSON inputs are capped at 64 MiB; other artefacts are streamed. See the
[receipt contract](https://github.com/DreamLab-AI/sealmap/blob/main/docs/REVIEW-EVIDENCE.md)
and the [worked observation](https://github.com/DreamLab-AI/sealmap#turn-a-finding-into-a-checked-evidence-receipt).

Exit codes: 0 success within the command's scope; 1 failed check, refusal or
unresolved work under the requested policy; 2 usage, parsing or IO error.

## Library

```toml
[dependencies]
sealmap = { version = "=0.4.0", default-features = false }
```

Disabling default features omits the CLI and parallel extraction. Add `parallel`
if required. The public modules re-export the focused crates:

| Module | Crate |
|---|---|
| `model` | `sealmap-model` |
| `rust` | `sealmap-rust` |
| `extract` | `sealmap-extract` |
| `mermaid` | `sealmap-mermaid` |
| `dense` | `sealmap-dense` |
| `corpus` | `sealmap-corpus`, including `pack` and `review` |


```rust,no_run
use std::path::Path;

let corpus = sealmap::generate_dir(Path::new("."), &sealmap::Options::default())?;
let report = sealmap::corpus::write(Path::new(".sealmap"), &corpus)?;
println!("{} files updated", report.entries.len());
# Ok::<(), std::io::Error>(())
```

Ids and fingerprints, on sources held in memory:

```rust
use sealmap::model::{Fingerprint, SymbolId};
use sealmap::{SourceSet, rust};

let mut src = SourceSet::new();
src.insert("src/lib.rs", "pub struct Db;\nimpl Db { pub fn get(&self) -> u8 { 1 } }").unwrap();
let model = rust::extract(&src, &rust::RustOptions { name: "shop".into(), ..Default::default() }).codebase;

let id = SymbolId::parse("sym:cargo shop . Db#get().").unwrap();
let get = model.symbol(&id).unwrap();
assert!(get.sig_hash.to_string().starts_with(Fingerprint::PREFIX));

// Reformatting and comments change neither fingerprint.
let mut again = SourceSet::new();
again.insert("src/lib.rs", "pub struct Db;\n\n// note\nimpl Db {\n    pub fn get(&self) -> u8 {\n        1\n    }\n}\n").unwrap();
let model2 = rust::extract(&again, &rust::RustOptions { name: "shop".into(), ..Default::default() }).codebase;
let get2 = model2.symbol(&id).unwrap();
assert_eq!((get.sig_hash, get.body_hash), (get2.sig_hash, get2.body_hash));
```

The `corpus::review` API checks receipt contracts in memory; library callers
verify referenced artefact bytes themselves. The CLI adds filesystem and
SHA-256 checks. See [sealmap-corpus](https://docs.rs/sealmap-corpus).

## Experimental seals

`seal sign`, `verify`, `seal-check` and `stale` track cited symbol/prose hashes.
They remain experimental: unchanged symbol hashes did not reliably establish
that a topic stayed true in the project's staleness experiments. Use `resolve`
for symbol lookup and conservative file-based selection for review queues.
Historical findings and limitations remain in the
[evidence corpus](https://github.com/DreamLab-AI/sealmap/tree/main/docs/evidence).

## Licence

Licensed under either of [Apache License, Version 2.0](https://github.com/DreamLab-AI/sealmap/blob/main/LICENSE-APACHE) or [MIT licence](https://github.com/DreamLab-AI/sealmap/blob/main/LICENSE-MIT), at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
