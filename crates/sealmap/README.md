# sealmap

The `review init`, `review check` and `review summary` commands validate
evidence-backed finding dispositions without modifying the original review.
They distinguish an inconclusive reproduction from a rejection, verify retained
artefact hashes, and report repair delivery stages separately. See the
[receipt workflow](https://github.com/DreamLab-AI/sealmap/blob/main/docs/REVIEW-EVIDENCE.md).

[![crates.io](https://img.shields.io/crates/v/sealmap.svg)](https://crates.io/crates/sealmap)
[![docs.rs](https://img.shields.io/docsrs/sealmap)](https://docs.rs/sealmap)

Deterministic code maps for LLM development harnesses: the facade and
command-line tool of [sealmap](https://github.com/DreamLab-AI/sealmap).

sealmap reads source without compiling it, builds a language-neutral model
(symbols, relations, the ordered call flow of every function) and projects it
into Mermaid diagrams with one document per source file. The same sources
always give byte-identical output. Every symbol has a stable `sym:` id (no
file path) and two fingerprints: `sig_hash` for its contract, `body_hash` for
its implementation, both blind to formatting, comments and moves.

Hand-written diagram topics cite those ids, and a lockfile, `seals.lock`,
records the hashes each cited symbol had when the topic was reviewed.
`sealmap verify` checks every seal against the code in CI, with no LLM
involved, and `sealmap stale` lists only the topics whose sealed symbols
changed. `sealmap dense` writes a compact agent projection of the code, and
`sealmap pack` a bounded review pack of chosen topics with the code they
cite (see the
[design](https://github.com/DreamLab-AI/sealmap/blob/main/docs/DESIGN.md)).

| Crate | Role |
|---|---|
| [`sealmap-model`](https://crates.io/crates/sealmap-model) | language-neutral code model |
| [`sealmap-mermaid`](https://crates.io/crates/sealmap-mermaid) | typed Mermaid writers and injective diagram ids (no dependencies without the default `model` feature) |
| [`sealmap-extract`](https://crates.io/crates/sealmap-extract) | logic shared by every language adapter (replaces `sealmap-frontend`, now a deprecated forwarding shim) |
| [`sealmap-rust`](https://crates.io/crates/sealmap-rust) | Rust language adapter (syn) |
| [`sealmap-corpus`](https://crates.io/crates/sealmap-corpus) | projections, index and write; the seal lock, `verify`, `seal_check`, `stale`, `resolve`, `sign`; review packs (`pack`) |
| [`sealmap-dense`](https://crates.io/crates/sealmap-dense) | compact agent projection: skeletons, call trees, short-name index, budgeted slices (`sealmap::dense`) |

## Command line

```sh
cargo install sealmap

sealmap generate .                  # write the corpus, model and index to .sealmap/ (gitignore it)
sealmap generate . --check          # exit 1 if .sealmap/ no longer matches a fresh generation
sealmap model    .  > model.json    # just the model
sealmap generate --repo api=../api --repo core=../core -o .sealmap   # several repositories as one
sealmap dense    . --stats          # the agent projection: dense.txt + _index.txt in .sealmap/dense

# Seals over the authored topics in docs/diagrams/ (lock: docs/diagrams/seals.lock)
sealmap resolve 'sym:cargo shop . db/Db#insert().'      # span and hashes, or absent + rename candidates
sealmap seal sign CP-03 --reviewer zai:glm-5.3 --model claude:sonnet   # seal one topic
sealmap verify                      # the CI gate: exit 1 unless every seal holds
sealmap seal-check CP-03            # classify chosen lock entries
sealmap stale --since main          # sealed symbols changed since a revision; exit 0

# Review packs: topics, dense slices of the code they cite, source windows
sealmap pack CP-03 CP-07 --budget 200000             # stdout; exit 1 naming each topic's size if over
sealmap pack --diff main --budget 200000 --shard -o packs-new/   # per-file selection, fresh output directory
```

Exit codes: 0 success; 1 a check failed, an id is not found, or a seal was
refused; 2 usage or IO error.

## Library

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

Sealing in memory, and catching a behaviour change:

```rust
use sealmap::corpus::seal::{self, Class, Lock, Signature};
use sealmap::model::SourcePath;
use sealmap::{SourceSet, rust};

let opts = rust::RustOptions { name: "shop".into(), ..Default::default() };
let model = |body: &str| {
    let mut src = SourceSet::new();
    src.insert("src/lib.rs", format!("pub struct Db;\nimpl Db {{ pub fn get(&self) -> u8 {{ {body} }} }}")).unwrap();
    rust::extract(&src, &opts).codebase
};
let file = SourcePath::new("store/01-db.md").unwrap();
let topic = "---\nid: ST-01\n---\nReads go through `sym:cargo shop . Db#get().`.\n";
let who = Signature { reviewer: "zai:glm-5.3".into(), model: "claude:claude-sonnet-5-5".into(), date: "2026-10-05".into() };

let mut lock = Lock::new();
let sealed = seal::sign(&mut lock, &file, topic, &model("1"), seal::LOCK_FILE, &who).unwrap();
let topics = [(file, sealed)].into_iter().collect();
let lock_text = lock.to_toml();

assert!(seal::verify(Some(&lock_text), seal::LOCK_FILE, &topics, &model("1")).passes());
let report = seal::verify(Some(&lock_text), seal::LOCK_FILE, &topics, &model("2"));
assert_eq!(report.failures().next().unwrap().class, Class::Behaviour);
```

## Licence

Licensed under either of [Apache License, Version 2.0](https://github.com/DreamLab-AI/sealmap/blob/main/LICENSE-APACHE) or [MIT licence](https://github.com/DreamLab-AI/sealmap/blob/main/LICENSE-MIT), at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
