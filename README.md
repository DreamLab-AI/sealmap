# sealmap

**Inspect Rust code and keep review evidence tied to the bytes you examined.**

[![crates.io](https://img.shields.io/crates/v/sealmap.svg)](https://crates.io/crates/sealmap)
[![docs.rs](https://img.shields.io/docsrs/sealmap)](https://docs.rs/sealmap)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange)](Cargo.toml)
[![Licence](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#licence)

Sealmap is a standalone Rust CLI and library. It reads Rust source without
compiling it, extracts symbols and call flows, and produces Mermaid diagrams,
compact text views and bounded review packs. It also checks evidence receipts
beside existing review exports, so another person or agent can see what was
observed, what remains unresolved and which repair stages have evidence.

No account, model service, Agentbox installation or database is required. The
libraries make no network calls. The CLI uses Git and tar for revision-based
comparisons; it never executes commands recorded in a review receipt.

| You want to… | Start with |
|---|---|
| Explore an unfamiliar Rust project | `sealmap dense . --stats` |
| Read generated architecture and call diagrams | `sealmap generate .` |
| Inspect symbols and relations as JSON | `sealmap model .` |
| Find the current span of a stable symbol | `sealmap resolve 'sym:cargo checkout . total().'` |
| Prepare selected topics and their cited code for review | `sealmap pack TOPIC-01 --budget 200000` |
| Keep review findings, observations and delivery claims distinct | `sealmap review init`, `check` and `summary` |

## Install

Examples below target **0.4.0**. Install the pinned package or build from a
checkout of this repository.

```sh
# Released package, pinned for reproducibility:
cargo install sealmap --version 0.4.0 --locked

# Or, from a checkout of this repository:
cargo install --path crates/sealmap --locked

sealmap --version
```

Rust 1.85 or later is required to build. The walkthrough uses Git; its receipt
example also uses `jq`, `sha256sum` and a Rust toolchain. These are example tools,
not dependencies of the installed Sealmap binary. Cargo's default `cli` feature
builds the command; library consumers can disable default features.

## Walk through a small project

This fixture deliberately contains a bug: `total` counts prices instead of
adding them. A valid control and a failing regression let us demonstrate the
review workflow with observations you can reproduce.

```sh
mkdir sealmap-demo
cd sealmap-demo
mkdir -p src docs/diagrams/checkout

cat > Cargo.toml <<'TOML'
[package]
name = "checkout"
version = "0.1.0"
edition = "2024"

[workspace]
TOML

cat > src/lib.rs <<'RUST'
pub fn total(prices: &[u32]) -> u32 {
    count_prices(prices)
}

fn count_prices(prices: &[u32]) -> u32 {
    prices.len() as u32
}

#[cfg(test)]
mod tests {
    use super::total;

    #[test]
    fn valid_control() {
        assert_eq!(total(&[1, 1]), 2);
    }

    #[test]
    fn totals_prices() {
        assert_eq!(total(&[2, 3]), 5);
    }
}
RUST

cat > docs/diagrams/checkout/01-total.md <<'TOPIC'
---
id: SHOP-01
title: Checkout total
sources:
  - src/lib.rs
---
The checkout total is calculated by `sym:cargo checkout . total().`.
Review whether it adds the prices or counts them.
TOPIC

printf '/target/\n/.sealmap/\n/evidence/\n/packs/\n' > .gitignore
cargo generate-lockfile --offline
git init --quiet
git add Cargo.toml Cargo.lock src docs .gitignore
git -c user.name='Sealmap demo' -c user.email='demo@example.invalid' \
  commit --quiet -m 'Add review fixture'
```

Generate views and resolve the function:

```sh
sealmap generate .
sealmap generate . --check
sealmap dense . --stats
sealmap model . > .sealmap/model.json
sealmap resolve 'sym:cargo checkout . total().'
```

Open `.sealmap/src/lib.rs.md` for the generated diagrams,
`.sealmap/dense/dense.txt` for the compact view and
`.sealmap/dense/_index.txt` for its symbol index. The model is machine-readable;
`resolve` prints the current location and signature/body hashes. Add `--tests`
when you want test code included in extraction.

Generated files belong in an ignored directory. Keep authored topic narratives
and review evidence separately: regeneration should not replace those judgements.

### Build a review pack

```sh
mkdir packs
sealmap pack SHOP-01 --budget 20000 -o packs/initial.txt

# A changed comment leaves symbol hashes alone, but its cited file changed.
printf '\n// Review the pricing contract.\n' >> src/lib.rs
sealmap pack --diff HEAD --budget 20000 -o packs/changed.txt

# Commit this demonstration edit before recording the test evidence below.
git add src/lib.rs
git -c user.name='Sealmap demo' -c user.email='demo@example.invalid' \
  commit --quiet -m 'Clarify the review scope'
```

A pack contains each selected topic verbatim, a dense slice around its `sym:`
citations, unresolved citations and bounded source windows. Whole-pack and shard
budgets are in **bytes**, not tokens. Oversized requests are refused with sizes;
source windows are deliberately bounded and labelled when clipped.

`--diff REV` uses a conservative file-based queue. A change anywhere in a cited
file retains its topics; declared `sources` can include non-Rust files. Build,
configuration and dependency changes widen the queue. Missing or unattributed
sources remain selected even when no local change is known. This selection does
not establish that a topic is wrong, or discover dependencies nobody recorded.

Topics using only `path:line` citations are still included as text, but dense
slices and source windows require `sym:` citations. Symbol-level `stale` remains
an experimental diagnostic and is not the default review selection rule.

For several topics, split at whole-topic boundaries:

```sh
sealmap pack SHOP-01 --budget 20000 --shard -o packs/batch-001
```

Every output destination must be fresh: `-o` refuses an existing file, and
`--shard -o` refuses an existing directory. No old shards are mixed into a new
run. An empty `--diff` selection with `-o` returns **1**, leaves existing output
untouched and reports that no pack was produced. Without `-o`, an empty selection
returns 0 with no pack on stdout. Respect the exit status; a failed write can
leave partial newly created output.

### Read several repositories together

```sh
# Substitute your own repository paths and topic id.
sealmap generate --repo api=../api --repo core=../core -o .sealmap/combined
sealmap pack API-01 --repo api=../api --repo core=../core \
  --budget 200000 -o packs/api-core.txt
```

Repository names become path prefixes; duplicate or overlapping names are
refused. Packs sort repository arguments and record each code repository's Git
baseline, relevant dirty/untracked paths, the corpus revision and extraction
options. Format 2 also records a BLAKE3 digest of all normalised extraction inputs
and separate local lockfile/toolchain/build-context hashes.

These fields identify the inspected inputs. They do not establish that a
consumer compiled those inputs or that a deployment runs them. Captures read a
working tree and are not atomic snapshots of repositories changing concurrently.
`--diff` and `stale --since` currently compare one Git repository and cannot be
combined with `--repo`.

## Turn a finding into a checked evidence receipt

Continue inside `sealmap-demo`. Start with an ordinary JSON review export; only
a unique, non-empty string `id` is required for each finding. The remaining
fields belong to your reviewer and remain unchanged.

```sh
mkdir evidence
cat > evidence/findings.json <<'JSON'
[
  {
    "id": "SHOP-01:total",
    "title": "Checkout counts prices instead of adding them",
    "status": "unverified"
  }
]
JSON

sealmap review init evidence/findings.json --out evidence/receipts.json
sealmap review check evidence/receipts.json --json
sealmap review summary evidence/receipts.json
```

After initialisation, the receipt is structurally valid, with one unresolved finding. To make
unresolved work fail a handoff or CI check:

```sh
# Returns 1 at this point: the finding is still unverified.
sealmap review check evidence/receipts.json --require-resolved
```

Now execute the control and regression, retaining both outputs. The regression
is expected to fail; inspect its assertion rather than treating any non-zero
exit as evidence of the claimed bug.

```sh
cargo test --offline --locked valid_control > evidence/test.log 2>&1
if cargo test --offline --locked totals_prices >> evidence/test.log 2>&1; then
  printf 'Expected regression failure did not occur.\n' >&2
  exit 1
fi
cat evidence/test.log
sha256sum evidence/test.log
```

Check that the control passes and the regression reaches the equality assertion
with actual value 2 and expected value 5. If compilation or setup failed instead,
record an `inconclusive` assessment and its `setup-failure` observation.

After checking that output, attach the actual revision, capture time and digest:

```sh
jq \
  --arg revision "$(git rev-parse HEAD)" \
  --arg captured "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg digest "$(sha256sum evidence/test.log | cut -d ' ' -f 1)" \
  '.dispositions[0] |= . + {
    status: "confirmed",
    reason: "The control passes; the regression reaches the incorrect total.",
    evidence: [{
      kind: "execution",
      outcome: "product-failure",
      captured_at: $captured,
      repository: "sealmap-demo",
      revision: $revision,
      configuration: "Local checkout fixture; default features; offline, locked Cargo.",
      command: "cargo test --offline --locked valid_control; cargo test --offline --locked totals_prices",
      artifact: {path: "test.log", sha256: $digest},
      observed: "valid_control passes; totals_prices fails with actual 2, expected 5.",
      positive_control: "valid_control checks the supported [1, 1] case in the same transcript."
    }]
  }' evidence/receipts.json > evidence/receipts.next.json
mv evidence/receipts.next.json evidence/receipts.json

sealmap review check evidence/receipts.json --json --require-resolved
sealmap review summary evidence/receipts.json
```

This confirms the finding in the recorded fixture. No repair or deployment has
been claimed, so its delivery list remains empty. Changing `test.log` or the
original `findings.json` now makes the receipt check fail its recorded digest.

A successful check validates **receipt consistency and artefact integrity**. It does
not independently establish that the observation is true, that an authority is
authorised, or that the system is secure. It never runs the recorded command.

| Disposition | Meaning |
|---|---|
| `unverified` | No completed assessment; missing dispositions also count here. |
| `inconclusive` | Evidence or access is insufficient; this is not a false positive. |
| `confirmed` | Scoped product failure supported by execution or source evidence. |
| `rejected` | A counterexample contradicts the claim; failing to reproduce it is insufficient. |
| `known` | An explicit accepted risk, with a decision record and authority. |

Record repair stages independently: `source`, `tested`, `merged`, `published`,
`adopted` and `deployed`. Each needs its own supporting observation. Tests and
runtime deployment require execution evidence; adoption also requires the
consumer version and lock or packaging artefact. A confirmed finding remains
confirmed after repair. Stage counts are attestations, **not a completion
percentage**.

Your evidence root defaults to the receipt's parent directory. All referenced
files must remain beneath it; `--evidence-root DIR` selects another root. JSON
inputs are capped at 64 MiB, artefacts are hashed as streams, and malformed
references, unknown receipt fields, missing files and altered bytes are refused.
Use this as a local checker; it does not sandbox concurrent filesystem changes.
See [the receipt contract](docs/REVIEW-EVIDENCE.md) for dirty-tree snapshots,
consumer boundaries, risk acceptance and delivery examples.

## Use the libraries

Pin the facade for extraction and generated views:

```toml
[dependencies]
sealmap = { version = "=0.4.0", default-features = false }
```

```rust,no_run
use std::path::Path;

let corpus = sealmap::generate_dir(Path::new("."), &sealmap::Options::default())?;
let report = sealmap::corpus::write(Path::new(".sealmap"), &corpus)?;
println!("{} generated files changed", report.entries.len());
# Ok::<(), std::io::Error>(())
```

Or depend on the part you need:

| Crate | Purpose |
|---|---|
| [`sealmap-model`](crates/sealmap-model) | Language-neutral model, source sets, stable ids and hashes. |
| [`sealmap-rust`](crates/sealmap-rust) | Rust parsing and name/receiver resolution. |
| [`sealmap-extract`](crates/sealmap-extract) | Shared extraction and flow-lowering machinery. |
| [`sealmap-dense`](crates/sealmap-dense) | Compact skeletons, call trees and budgeted slices. |
| [`sealmap-mermaid`](crates/sealmap-mermaid) | Typed Mermaid writers; no dependencies without default features. |
| [`sealmap-corpus`](crates/sealmap-corpus) | Generated corpus, packs, evidence receipt types/checks and experimental seals. |

For receipts alone, `sealmap-corpus = "=0.4.0"` exposes
`review::{findings, validate, artifacts, validate_artifact}` and the serde types.
Library callers check contracts in memory and must also verify artefact bytes.
The CLI supplies filesystem confinement and SHA-256 checking. Library examples
are in the [facade README](crates/sealmap/README.md),
[corpus README](crates/sealmap-corpus/README.md) and [rustdoc](https://docs.rs/sealmap).

## Limits and evidence

Rust is the implemented language adapter. Extraction does not compile or
type-check: name/receiver heuristics can produce inferred edges, and dependency
or macro-generated code may remain unresolved. Views distinguish `exact`,
`inferred` and `external`; those labels describe resolution, not proof of
runtime behaviour. Generated diagram rendering is a separate concern from
whether an authored narrative is correct.

Historical experiments remain part of the project:

| Finding | Evidence and limit |
|---|---|
| Dense views were 0.17–0.29× source size on the measured corpora. | [Dense projection](crates/sealmap-dense); a size result, not a universal review-quality claim. |
| Finer-than-file staleness missed real changes. | [E0d](docs/evidence/E0d/RESULTS.md): per-file selection caught all 29 labelled stale pairs in a 60-pair sample; symbol hashes caught 0.76. This is why selection remains conservative. |
| Batching and model triage reduced projected upkeep. | [EH](docs/evidence/EH/RESULTS.md); historical corpus/model results, not a guarantee for another estate. |
| Sequence-only rewriting lost information and invented claims. | [EK](docs/evidence/EK/RESULTS.md), [ES](docs/evidence/ES/RESULTS.md), [ES2](docs/evidence/ES2/RESULTS.md). Keep useful diagram types and check their claims against source. |
| Packs did not beat full source when the source fit in context. | [ER](docs/evidence/ER/RESULTS.md), [ER-glm](docs/evidence/ER-glm/RESULTS.md); larger-system benefit remains unestablished by those experiments. |
| Reading generated views found real defects. | [Consult record](docs/evidence/consult/2026-10-06-codex.md), [changelog](CHANGELOG.md). This supports using them as an inspection aid. |

### Seals remain experimental

A `sym:` id identifies a definition without encoding its file path, for example
`'sym:cargo checkout . total().'`. Signature and body fingerprints ignore
formatting and comments. `resolve` is useful independently of seals.

`seal sign`, `verify`, `seal-check` and `stale` retain their tests, but a seal
whose hashes still hold does not prove that a topic remains true. Seal signing
records a reviewer and authoring model and requires different known families;
it does not authenticate a model service or verify its claimed review. See the
[design](docs/DESIGN.md) and [seal API](https://docs.rs/sealmap-corpus/latest/sealmap_corpus/seal/).

<details>
<summary><strong>Place in an ecosystem: authored docs, renderers and the Agentbox use case</strong></summary>

Sealmap can be used from a terminal, a CI job, a coding assistant or a Rust
application. It does not own review scheduling, model credentials, deployment
or issue management. A consumer can keep those policies while calling the same
CLI or library for deterministic extraction and evidence checks.

In the DreamLab-AI estate, the pieces have separate jobs:

- Authored diagram topics carry narratives, architectural decisions and findings.
  They are reviewed against source; generated output does not replace them. The
  estate keeps them in one master record, VisionFlow `docs/diagrams`; sealmap's own
  are its `sealmap` area there.
- Sealmap supplies Rust views, stable symbol locators, bounded packs and receipt
  validation. Its packs may contain source, so choose recipients accordingly.
- [diagram-ir](https://github.com/DreamLab-AI/diagram-ir) and
  [Pele](https://github.com/obsidianmd/pele) provide separate diagram-processing
  and rendering capabilities. Successful parsing/rendering does not certify a
  diagram's meaning.
- Agentbox's `sealmap-review` skill schedules corpus triage and external reviews,
  preserving their packs and replies. Its external mode sends diagrams only;
  local inline review can inspect source. These are adapter policies.

From an Agentbox checkout with Sealmap 0.4.0 available:

```sh
C=skills/sealmap-review/scripts/review-cadence.cjs
node "$C" status
node "$C" receipts init docs/review/findings.json \
  --out docs/review/receipts.json
node "$C" receipts check docs/review/receipts.json --json --require-resolved
node "$C" receipts summary docs/review/receipts.json
```

Receipt subcommands delegate to the Rust CLI and preserve its exit status;
there is no second evidence validator in the adapter. See the
[Agentbox skill](https://github.com/DreamLab-AI/agentbox/blob/main/skills/sealmap-review/SKILL.md)
for discovery, privacy, review provenance, scheduling and spend controls.
Other consumers can adopt the core without those services or policies.

</details>

## Exit codes and development

| Code | Meaning |
|---|---|
| `0` | Command succeeded within its stated scope. A receipt may still have unresolved findings unless `--require-resolved` was used. |
| `1` | Check failed, requested symbol absent, pack/seal refused, or no topics selected for an explicit output path. |
| `2` | Usage, parsing or IO error; an existing pack destination is an IO refusal. |

`generate --check` compares generated files with a fresh generation. `review
check` validates a receipt. `verify` checks experimental seals. Their names do
not make these interchangeable checks.

Workspace gates include formatting, clippy, default/no-default-feature
tests, rustdoc, deterministic generation and rendered-diagram checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace --no-default-features
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

See [CI](.github/workflows/ci.yml), [design decisions](docs/DESIGN.md),
[receipt documentation](docs/REVIEW-EVIDENCE.md),
[architectural diagrams](https://github.com/DreamLab-AI/VisionFlow/tree/main/docs/diagrams/sealmap) and [release history](CHANGELOG.md).

## Licence

Dual licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. Contributions are accepted under the same terms unless stated otherwise.
