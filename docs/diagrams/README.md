# Diagrams as code — moved to the VisionFlow estate master record

The hand-authored, citation-verified corpus that lived here (21 topics in six areas, the
register and the decisions timeline) moved on 2026-11-05 into the estate's single diagram
corpus, where it is the `sealmap` area:

- tree: <https://github.com/DreamLab-AI/VisionFlow/tree/main/docs/diagrams/sealmap>
  (local checkout: `../VisionFlow/docs/diagrams/sealmap`)
- gate: `node scripts/diagram-index-gen.cjs docs/diagrams --check --strict-citations`, run from
  the VisionFlow root. `sources:` there reach this repository as `../sealmap/…`, and every
  topic keeps the `verified_commit` it was stamped at here.
- the narratives and register paragraphs moved intact, and the master record cite-checks
  their prose as it does the diagrams.

This directory is still sealmap's default `--diagrams` location for **other** repositories
that adopt it (`sealmap verify`, `seal`, `pack`); only this repository's own corpus moved.
The Pele build that `tools/validate-mermaid.mjs` renders with moved to `tools/vendor/pele`.
History up to the move is in this repository's git log.

## Id map

Topic ids were renumbered into the master record's one-prefix-per-area scheme. Diagram ids
follow their topic (`COR-04.3` is now `SM-15.3`). Review files under `docs/review/` and
`docs/evidence/` keep the ids they were written with.

| Old | New | Was | Now (VisionFlow) |
|---|---|---|---|
| MOD-01 | SM-01 | `docs/diagrams/model/01-the-sym-id-grammar.md` | `docs/diagrams/sealmap/01-the-sym-id-grammar.md` |
| MOD-02 | SM-02 | `docs/diagrams/model/02-content-hashes-and-fingerprints.md` | `docs/diagrams/sealmap/02-content-hashes-and-fingerprints.md` |
| MOD-03 | SM-03 | `docs/diagrams/model/03-the-codebase-model-and-schema-v2.md` | `docs/diagrams/sealmap/03-the-codebase-model-and-schema-v2.md` |
| EXT-01 | SM-04 | `docs/diagrams/extract/01-the-adapter-pipeline.md` | `docs/diagrams/sealmap/04-the-adapter-pipeline.md` |
| EXT-02 | SM-05 | `docs/diagrams/extract/02-minting-ids-for-definitions.md` | `docs/diagrams/sealmap/05-minting-ids-for-definitions.md` |
| EXT-03 | SM-06 | `docs/diagrams/extract/03-the-flow-walker-and-raw-ir.md` | `docs/diagrams/sealmap/06-the-flow-walker-and-raw-ir.md` |
| EXT-04 | SM-07 | `docs/diagrams/extract/04-lowering-confidence-and-labels.md` | `docs/diagrams/sealmap/07-lowering-confidence-and-labels.md` |
| EXT-05 | SM-08 | `docs/diagrams/extract/05-workspace-name-resolution.md` | `docs/diagrams/sealmap/08-workspace-name-resolution.md` |
| EXT-06 | SM-09 | `docs/diagrams/extract/06-signature-and-body-fingerprints.md` | `docs/diagrams/sealmap/09-signature-and-body-fingerprints.md` |
| MER-01 | SM-10 | `docs/diagrams/mermaid/01-typed-writers-and-escaping.md` | `docs/diagrams/sealmap/10-typed-writers-and-escaping.md` |
| MER-02 | SM-11 | `docs/diagrams/mermaid/02-injective-diagram-ids.md` | `docs/diagrams/sealmap/11-injective-diagram-ids.md` |
| COR-01 | SM-12 | `docs/diagrams/corpus/01-generating-the-corpus-and-index.md` | `docs/diagrams/sealmap/12-generating-the-corpus-and-index.md` |
| COR-02 | SM-13 | `docs/diagrams/corpus/02-structure-sequence-and-overview-projections.md` | `docs/diagrams/sealmap/13-structure-sequence-and-overview-projections.md` |
| COR-03 | SM-14 | `docs/diagrams/corpus/03-generate-check-write-and-the-cli.md` | `docs/diagrams/sealmap/14-generate-check-write-and-the-cli.md` |
| COR-04 | SM-15 | `docs/diagrams/corpus/04-seals-the-lock-and-the-checks.md` | `docs/diagrams/sealmap/15-seals-the-lock-and-the-checks.md` |
| COR-05 | SM-16 | `docs/diagrams/corpus/05-the-seal-commands.md` | `docs/diagrams/sealmap/16-the-seal-commands.md` |
| COR-06 | SM-17 | `docs/diagrams/corpus/06-review-packs.md` | `docs/diagrams/sealmap/17-review-packs.md` |
| COR-07 | SM-18 | `docs/diagrams/corpus/07-review-evidence-and-selection.md` | `docs/diagrams/sealmap/18-review-evidence-and-selection.md` |
| DEL-01 | SM-19 | `docs/diagrams/delivery/01-ci-msrv-and-determinism.md` | `docs/diagrams/sealmap/19-ci-msrv-and-determinism.md` |
| DEL-02 | SM-20 | `docs/diagrams/delivery/02-design-versus-code.md` | `docs/diagrams/sealmap/20-design-versus-code.md` |
| DEN-01 | SM-21 | `docs/diagrams/dense/01-the-dense-agent-projection.md` | `docs/diagrams/sealmap/21-the-dense-agent-projection.md` |
