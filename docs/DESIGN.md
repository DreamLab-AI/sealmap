# sealmap design

## Review evidence and selection (2026-11-03)

The estate audit exposed gaps in review bookkeeping, rather than new evidence
for symbol-level skipping. `pack --diff` now selects conservatively by source
file, with configuration/dependency changes and unknown coverage widening the
selection. Pack format 2 records the normalised extraction input digest and
the CLI's repository/build-context provenance. Those records identify inputs;
they do not attest that a runtime consumed them.

`sealmap_corpus::review` and `sealmap review init/check/summary` validate adjacent
finding dispositions and artefact integrity. They distinguish setup failure,
inconclusive evidence and counterevidence, and keep every repair delivery stage
independent. [The receipt contract](REVIEW-EVIDENCE.md) and
[expectations](REVIEW-EXPECTATIONS.md) define the supported workflow.
Agentbox's existing review skill consumes it; there is no new issue database.

These changes supersede the old symbol-only `--diff` selection below, while
preserving the experimental status of seals and the evidence programme's
negative results. Agreement between reviewers remains a ranking signal. The
new tooling does not establish that diagram packs outperform full-source
review, or that an artefact hash verifies the truth of its contents.

## Status after the evidence programme (2026-10-06)

The design below was accepted on 2026-10-05 and is kept as written, as the
record of what was decided and why. Its central bet was that symbol-granular
seals would make a hand-written corpus cheap to keep true. The pre-registered
experiments in [`evidence/`](evidence) tested that bet, and it did not hold.
This section governs where it and the text below disagree.

**Kept: the code-lens surface.** Extraction (`sealmap-extract`,
`sealmap-rust`), the model and its `sym:` ids and hashes, the generated Mermaid
views, `sealmap-dense`, `pack` and `generate --check`. Reading the generated
views of sealmap's own code caught 2 regressions, 5 resolver bugs and 1
projection bug that the tests had missed ([`evidence/consult/`](evidence/consult/2026-10-06-codex.md));
`dense` is 0.17–0.29× source; `pack` is deterministic and refuses rather than
truncates. Further work goes here.

**Frozen: seals and fine-grained staleness.** `seal sign`, `verify`,
`seal-check`, `stale` and `resolve` stay in the crates and the CLI with their
tests, documented as experimental and not recommended for corpus upkeep. On
60 blind-labelled (topic, commit) pairs, per-file flags caught every stale
pair; per-symbol caught 0.76 (E0), per-region 0.66 (E0b), call flow 0.45 (E0c)
and changed-line overlap 0.38 (E0d), for at most about 2× fewer flags
([`evidence/E0d/ENDPOINT4.md`](evidence/E0d/ENDPOINT4.md)). The upkeep that
works is per-file flags batched weekly, triaged by a model (EH). §3's seal
workflow, §9's endpoint 4 ("cheaper upkeep") and §10's steps 4–6, 8 and 9
are closed. Step 4 ran as E0–E0d and step 8's review comparison as ER;
the `sealmap` skill, the dogfood seal of this corpus and the VisionFlow
migration will not be built.

**Dropped.**

- **`sealmap-ts`** (§8; §10 step 7). It was gated on finer staleness proving
  its worth, and it did not.
- **Sequence-only corpora.** Removing the other diagram kinds cost review
  recall (EK), and sequence-first rewrites invented facts (ES, ES2). Mixed
  corpora stay.
- **The corpus pack as a replacement for source in review** where the source
  fits in one context: full source did as well or better (ER, ER-glm). This
  does not settle repositories too large for one context.

---

Status: accepted by the owner, 2026-10-05 (all §11 recommendations taken). Supersedes the v0.1 README's
"committed 1:1 corpus with a drift gate" premise. Evidence: `research/01`–`05` in
the design session scratchpad. Each section below names the report it summarises.

## 1. What sealmap is

sealmap is a deterministic substrate under an LLM diagram skill. It does not
write the diagrams people read. It does the bookkeeping that makes them cheap
to keep true:

- it extracts a code model;
- it gives every symbol a stable id and two content hashes;
- it checks that reviewed diagrams still describe the code they were sealed
  against;
- it builds bounded review packs on demand.

The LLM skill does the consolidation, the narratives, the register and the
review that produces a seal.

> **Panel line (draft):** *the seal and the distillation do the work, not a
> bigger model.*

### Crate rule

A capability goes into a crate only if all three hold:

1. it must be deterministic;
2. it is useful outside this estate;
3. it is cheap to keep general.

Everything else lives in the skills.

## 2. Three layers

| Layer | Artefact | Committed | Written by | Gated by |
|---|---|---|---|---|
| **Authored** | `docs/diagrams/<area>/NN-*.md`: consolidated topics, narratives, register; citations are `sym:` ids | yes | LLM skill, tiered (§6) | `sealmap verify` (ids resolve) + `diagram-index-gen.cjs --check --render` |
| **Sealed** | `docs/diagrams/seals.lock` (and `docs/adr/seals.lock`); each topic and ADR carries a static `sealed: seals.lock` line | yes | the skill's seal step | `sealmap verify`: **the CI gate** |
| **Generated** | model, index, dense projection, optional 1:1 Mermaid, under `.sealmap/` | **never** (gitignored) | `sealmap generate`, `sealmap dense` | rebuilt in memory; never trusted from disk |
| **Review pack** | `sealmap pack`: pegged topics + dense slices + bounded source windows (built); `--review` = diagrams only (not built) | never | crate | Gemini external review, seal review, debugging |

**Retired from v0.1 (done in step 3):** the committed generated corpus
(`docs/codemaid/`, later `docs/sealmap/`) and its CI drift gate. The 1:1
writer stays as an optional view; `generate --check` compares a directory with
a fresh generation and writes nothing, and CI instead checks that two fresh
generations are byte-identical.

## 3. The seal

A seal records that **topic T's claims were reviewed against these exact symbol
versions**.

```toml
version = 1
algorithm = "sm1"
generator = "sealmap 0.2.0"

[[topic]]
id = "CP-03"
file = "control-plane/03-the-scr-executor.md"
topic_hash = "blake3-16:…"     # binds the prose: an unreviewed edit fails verify
reviewer = "zai:glm-5.3"       # exactly one host:model, the verdict-giver; sign refuses the author's family (ADR-2130)
model = "claude:claude-sonnet-5"
date = "2026-10-05"
symbols = [
  { id = "sym:npm control-plane . scr/executor/execute().", sig = "…", body = "…" },
]
```

`sign` takes `--reviewer` as one `host:model` (a multi-rung route is refused,
exit 1, nothing written); `--model` may be an `a -> b` route when an escalation
ran, and the reviewer's family is compared with every author rung. The family
of a `loom` route comes from the route's model id and is not checked against
what the Loom serves.

As built (`sealmap_corpus::seal`): the canonical form above is exact, with
`symbols` one inline table per line in id order (`symbols = []` when empty),
topics in id order, and `file` relative to the lock's directory.

- **Lock plus pointer.** Hashes live in the lock, so re-seals never rewrite
  authored prose and reviewers read one file. The static `sealed:` pointer keeps
  any topic self-describing. `verify` fails if the pointer and the lock
  disagree.
- **Canonical form.** The lock is sorted TOML serialised by the crate. Merge
  conflicts stay per topic. `sealmap seal sign <topic> --reviewer --model
  [--date]` re-derives an entry mechanically from the current code, writes the
  lock canonically and puts the pointer line in place; the skill's
  `seal.mjs sign` calls it after its own policy checks. Canonicality is
  checked by re-serialising: any other bytes are a lock fault.
- **`topic_hash`.** BLAKE3 of the topic text with line endings normalised and
  every front-matter `sealed:` line removed, first 16 bytes
  (`blake3-16:`). The pointer therefore never changes the hash; any other byte
  does.
- **Citations.** An inline code span that starts like a global id
  (`sym:<manager> …`, manager not `extern`) is a citation, outside front
  matter and fenced blocks (so not inside Mermaid). A mistyped one fails as an
  unsealed citation; `sym:?` and `sym:extern` forms are grammar mentions,
  never citations, since they name no definition.
- **Moves cost nothing.** `sym:` ids carry no line numbers, so a code move
  changes neither the lock nor the topic. The skill's index pass renders
  `path:line` at HEAD for human readers.

### What `verify` checks

| Condition | Class |
|---|---|
| id resolves, `sig` and `body` match | holds |
| `sig` same, `body` differs | behaviour |
| `sig` differs | contract |
| id gone; another id has the same `body` | absent (rename suspected) |
| file holding a sealed id fails to parse | unparsable, fail closed |
| `sym:` cited but not sealed (or not a canonical id) | unsealed citation |
| a lock entry with no file, or a sealed symbol its topic no longer cites | orphan |
| `topic_hash` differs | prose edited since seal |
| pointer and lock disagree, or the lock is not canonical | lock fault |

Everything except **holds** exits 1. Two decisions made in the build:

- **Fail closed without adapter diagnostics.** An unparsable file is modelled
  as its lone module symbol tagged `parse_error`; a sealed id that is that
  module, or is absent while one of its ancestor modules is, is unparsable,
  never absent. Rename candidates must share the sealed id's descriptor kind.
- **Coverage.** A topic with no seal and no citation (a legacy `path:line`
  topic) is reported as unsealed coverage and does not fail `verify`, so a
  corpus migrates one topic at a time.

Reviewer family, evidence and signatures are skill policy, enforced by
`seal-gate.mjs` next to `verify`. That keeps the crate estate-neutral.

## 4. Crate surface

| Crate | Owns |
|---|---|
| `sealmap-model` | language-neutral model; **`sym:` grammar** (SCIP-descriptor style, kind-explicit, no file path, version `.`); `sig_hash` / `body_hash` (an optional `flow_hash` was considered and not built: nothing needs it yet) (BLAKE3 over comment- and whitespace-insensitive token streams, after rustc fingerprints); schema v2 |
| `sealmap-extract` | (published as `sealmap-frontend` 0.1.0) shared by both language adapters: raw flow IR, flow normalisation, call aggregation, confidence lattice, label rules, id builder and hashing. Extracted from `-rust` first, with no behaviour change. This is the parity lever |
| `sealmap-rust` | syn language adapter, hardened (§7) |
| `sealmap-ts` | oxc language adapter (§8); **deferred** until E0-R shows the gain |
| `sealmap-mermaid` | typed writers, short participant aliases (−26.5 % tokens), **injective** ids with a uniqueness assertion |
| `sealmap-dense` | **exists.** Agent projection: skeletons with `L<start>-<end>` and short names, indented call trees (each callable expanded once; `^` reference, `↺` cycle, `…` depth cut), `_index.txt`, and the budgeted `slice` that `pack` embeds. Measured `dense.txt` 0.17–0.29× source and 13–18 tokens per call edge against 34–53 for Mermaid (sealmap, tokio, VisionClaw; bytes / 4) |
| `sealmap-corpus` | generate, the `seal` module (lock parse and canonical write), `resolve`, `seal-check`, `stale`, `verify`, `sign`; the `pack` module (review packs, sharding, change selection) |
| `sealmap` | facade and CLI; `sealmap-ts` behind a default feature, because oxc needs MSRV 1.97 |

**`sealmap-dense` as built (differs from the research estimate).** The
research figure (0.45× source) projected every function's flow on its own. The
crate instead draws trees from entry points and expands each callable once, so
every call site appears exactly once; that, plus printing each signature once,
brings `dense.txt` to 0.17–0.29× source. The `_index.txt` that resolves short
names to `sym:` ids is nearly as large again (0.15–0.17× source, because the
ids are long), so it is a separate file an agent loads only to resolve a name;
`dense.txt` plus the index is still 0.32–0.46×. Slices carry their own index
section. A tree header with no mark is an entry point (nothing internal calls
it); `name …` continues a cut and `name ↺` is reachable only through a cycle.
The depth limit barely moves the size (±3 % between 1 and 10 levels); the
default is 3.

### CLI

| Command | Does |
|---|---|
| `resolve <sym:…>` | current span and hashes, or `absent` plus rename candidates |
| `stale [--since rev]` | topics whose sealed symbols changed, with class; exit 0 |
| `seal-check [topic…]` | classify the given lock entries (default: all) |
| `verify` | `seal-check` across the corpus, plus coverage and lock faults |
| `seal sign <topic> --reviewer --model [--date]` | write one topic's seal from the current code |
| `pack [topic…] [--diff REV] [--budget BYTES] [--depth N] [--source-window LINES] [--shard -o DIR]` | deterministic blob. Refuses when over budget, naming each topic's size; `--shard` splits by topic instead; never silently truncates; never includes the lock |
| `generate [--check]` | writes `.sealmap/`; `--check` compares an existing directory with a fresh generation, writes nothing, exits 1 on drift |
| `dense [PATH] [-o DIR] [--depth N] [--stats]` | writes `dense.txt` and `_index.txt` (default `.sealmap/dense`) |
| `export --scip` (later) | interop |

Git stays out of the libraries. `pack::changed_topics` takes two models, and
`--since` and `--diff` are CLI sugar that shell out to `git archive` (piped to
`tar`) and read the revision as a second model. Seal commands take `-C ROOT`,
`--diagrams DIR` (default `docs/diagrams`) and `--json`.

**`pack` as built (differs from the plan above).**

- `--diff REV` compares one revision with the working tree, as `--since`
  does, rather than two named trees. It selects the topics `stale` reports
  for the lock against REV, plus every topic citing a symbol added, removed,
  made unparsable or rehashed since REV. Moves do not count. Topics named on
  the command line join the selection.
- The budget flag is `--budget`, not `--max-bytes`. The refusal names the
  header's size and each topic's. `--shard` fills numbered packs with whole
  topics, in id order, and refuses only a topic too large to fit alone.
- `--review` (diagrams only) is not built.
- The format is versioned text (`# sealmap-pack 1`). The header records the
  generator, codebase, revision (`HEAD`, plus `+dirty` when tracked files
  differ), any `diff:` base, the topics, the budget, the depth and the source
  window. Every block line gives its payload's length in bytes, so a reader
  can skip a block even when a topic contains a line that looks like one.
- Unresolved citations (`absent`, `unparsable`, `invalid`) are listed, never
  dropped. Source windows show at most N lines of each cited symbol, and a
  clipped window says so (`L40-79 of L40-112`).
- A topic that still cites `path:line` rather than `sym:` ids packs as its
  text alone.

Licence `MIT OR Apache-2.0`. Every crate is published to crates.io with full
rustdoc, a README, and a clean `cargo doc --no-deps`. Skills pin the published
version, never a path dependency.

## 5. Skills

| Skill | State | Role |
|---|---|---|
| `sealmap-review` | **shipped** (agentbox `c4ea4efca`, `800d5494a`) | External mode: diagrams only, single-shot to Gemini 3.8 Flash, high thinking, `critical` + `premortem` lenses, pack-first ordering for cache reuse. Inline mode: Claude subagent checks topics against cited code. Findings go to build-with-quality intake as hypotheses (test first, then confirmed / rejected / known). Trigger set: train 12/12, held-out 6/8, zero false triggers |
| `sealmap` | to build with skill-builder | Read the generated substrate; the seal workflow (`seal.mjs propose\|sign\|rehome`, `seal-gate.mjs`, `cite-to-sym.mjs` migration); tiers (`tier.mjs`); `edge-check.mjs` (diagram-ir + `resolve` catches invented edges); the A/B bench |
| `diagrams-as-code` | small amendment | `diagram-index-gen.cjs` accepts `sym:` and `sealed:`; `--cite-check` kept for legacy `path:line` topics; the researcher role becomes `resolve` (zero tokens) |
| `build-with-quality` | small amendment | step 5a **Re-seal**: `stale` → behaviour (cheap cross-family review, sign) / contract (ADR addendum, re-consolidate, review, sign) / absent (confirm rename). Code, topic and lock land in **one commit**, ending today's two-commit routine |

`sealmap pack` exists, so `sealmap-review` can swap its own packer for it in
inline mode, where code is wanted. Its external mode is diagrams only, which
waits for `--review`. Until the skill changes, it reads the corpus directly.

## 6. Model per step

Tiers come from the measured descent, not assumption. The skill names
`[model_routing]` activities and never names model ids.

| Step | Start | Escalate on |
|---|---|---|
| extraction, symbol → line, staleness, pack | **T0** sealmap (0 tokens) | — |
| diagram condensing, labels | **T1** Loom qwen3.8-27B / Haiku | render gate fails twice; diagram-ir finds an invented edge; > 25 % inferred calls |
| narratives, per-topic register | **T2** Sonnet | seal-review rejection; spans ≥ 3 areas |
| Tensions, ranking, cross-corpus synthesis | **T3** Opus | corpus too large → T4 Fable |
| seal review | **T3 cross-family** (GLM-5.3 if the author was Claude) | contract-class disagreement → T4 |
| per-commit maintenance | **T0** `stale`; behaviour → T2; contract → T3 | — |

De-escalation is sticky per corpus, in `docs/diagrams/.tiers.json`. It needs
four new activities (`diagram-polish`, `diagram-narrative`,
`diagram-synthesis`, `seal-review`) plus `loom` / `zai` hosts, so **an ADR**.

## 7. Rust hardening (done as a prototype; patch `research/03-rust-hardening.patch`)

| Fault | Cause | Fix |
|---|---|---|
| stack overflow on VisionClaw | deeply nested generics in vendored `typenum`, on rayon's 2 MiB stacks in debug | larger worker stacks, per-file panic guard, depth cap |
| 48 s and climbing | `.gitignore` ignored: 47,229 files walked, not 934 | respect ignore files |
| hang on older crates | exponential glob-import resolution (908 M calls on one file) | memoised table: 43 ms |
| id collision | `::` → `__` is not injective | injective readable ids derived from `sym:` ids (step 2; no hash suffix is needed) + corpus-wide assertion |
| wrong lane | `Path::parent()` drawn on `SymbolId` | resolver fix in the id work |

**Result:** VisionClaw generates in **1.0 s**, tokio in 0.24 s, with
byte-identical output across runs. Tests, clippy, fmt and docs are clean.

## 8. TypeScript language adapter

> **Deferred (owner decision, 2026-10-05).** `sealmap-ts` is built only if
> E0-R on the Rust repositories (§9, step 4 in §10) shows that the
> precise-staleness gain is real. The design below stands as written for
> that case.

- **Targets:** co-created / campaignbuilder (about 700 `.ts` / `.tsx`).
- **Parser:** oxc parser + semantic + resolver, versions pinned exactly.
- **Ids:** the same `sym:` grammar. Route handlers get synthesised ids
  (``index/`POST /api/provision`().``). Without them, the 52 anonymous handlers
  in `index.ts` stay file-granular, and E0's gain disappears.
- **Disambiguation:** two packages are both named `campaignbuilder`, so the
  package part carries a disambiguator.
- **Parity:** a shared conformance crate of about 40 dual-language cases, with
  a parity gate in CI.
- **Effort:** 31–36 engineer-days, about 4 weeks elapsed with Rust and TS in
  parallel after the `sealmap-extract` split.

## 9. Evidence plan

The endpoints are a fixed sequence. Each is tested only if the previous one
holds, and all are pre-registered in `PREREG.md` before any endpoint run.

1. **E0, deterministic maintenance (no LLM).** Replay the last 100 code
   commits. Count topics invalidated per commit:
   - **today:** file-level, the corpus's `sources:`;
   - **sealed:** symbol-level, plus a one-hop-callee variant.
   - **Baseline measured:** median 10/45, p90 40/45 on campaignbuilder, driven
     by `index.ts` (39 topics) and `compose.pod.yaml` (31).
   - **Reported:** the reduction ratio, coverage honesty (share falling back
     to file level), and a precision sample for real changes hidden by
     symbol-level gating.
   - **Order:** Rust first (VisionClaw, agentbox); TS after `sealmap-ts`.
2. **External review yield.**
   - **Measure:** diagrams-only critical + pre-mortem review, recall of a
     frozen issue list per 100k pack tokens, against handing over the code.
   - **Pilot (n = 1):** blind critical review rediscovered 7 of 30 Tensions
     plus 10 known register items. New claims: 1 real, 5 overstated, 0 wrong.
   - **Changed:** the owner's open prompt was dropped as the primary arm (it
     flatters); critical + pre-mortem are baked in.
3. **Non-inferiority.** The substrate does not cost review recall (δ = 0.05).
4. **Cheaper upkeep.** Maintenance tokens per commit, today against sealed and
   tiered, lower bound > 1×.

Planted faults in **code**, distilled into diagrams, test the distillation end
to end. **Dogfood:** sealmap's own repository, sealed, is round 0.

## 10. Order of work

1. **Repository.** Rename `DreamLab-AI/codemaid` → `sealmap`, rename the
   crates, change the licence to dual. Apply the hardening patch. Extract
   the shared extraction core, `sealmap-extract` (published as
   `sealmap-frontend` 0.1.0; renamed in step 2, because "frontend" read as a
   UI).
2. **Ids and hashes.** `sym:` grammar, `sig` / `body` hashes, injective
   Mermaid ids, schema v2.
3. **Seal surface.** `seal` module plus `resolve` / `stale` / `seal-check` /
   `verify` / `pack`, and `sealmap-dense`. Retire the committed corpus.
   **First crates.io publish (0.2).** *Status: done in the tree (the seal
   module, `resolve`, `stale`, `seal-check`, `verify`, `seal sign`, the
   retirement, `sealmap-dense`, `sealmap dense` and `pack`); published
   as 0.2.0.*
4. **E0-R** on VisionClaw and agentbox. This is the first headline number.
5. **`sealmap` skill** (skill-builder): seal workflow, tiers, edge-check,
   bench. Amend diagrams-as-code and build-with-quality. Write the routing ADR.
   Bake via `lib/sealmap.nix`.
6. **Dogfood.** Seal sealmap's own corpus. `verify` runs non-blocking in CI for
   2 weeks, then blocking.
7. **`sealmap-ts`**, **deferred**: built only if E0-R (step 4) shows the
   precise-staleness gain is real; then E0-T on campaignbuilder.
8. **Review A/B.** Calibrate, freeze `PREREG.md`, run the endpoint window
   (≥ 30 commits).
9. **Migrate VisionFlow** area by area: `cite-to-sym`, then review, then seal,
   with `verify` running non-blocking first.

## 11. Decisions (accepted 2026-10-05)

1. **`topic_hash`: yes.** A seal binds the topic's prose as well as its
   symbols; a cosmetic edit costs a zero-token re-seal.
2. **Lock plus pointer: yes.** Hashes live in `docs/diagrams/seals.lock`; each
   topic carries a one-line pointer in its front matter.
3. **Repository renamed** to `DreamLab-AI/sealmap` (done). **Crate names are
   reserved by a real, documented release** of the renamed 0.1 crates at the
   end of step 1, never by placeholder crates. Crates that do not exist yet
   (`sealmap-dense`, `sealmap-ts`, `sealmap-extract` if not split by then)
   are published when they have content.
4. **Model-routing ADR: yes.** Four activities (`diagram-polish`,
   `diagram-narrative`, `diagram-synthesis`, `seal-review`) and `loom` / `zai`
   hosts, recorded in agentbox.
5. **Gemini key rotated.** The new key goes in agentbox `.env` as
   `GEMINI_API_KEY`; no key is ever written to a repository.
