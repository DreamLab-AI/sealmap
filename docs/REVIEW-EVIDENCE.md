# Evidence-backed review dispositions

`sealmap review` keeps an original review export intact and checks a separate
receipt. It is useful after an external audit, when reproductions fail during
setup, when a producer fix has several consumers, and when handing unfinished
work to another agent. It needs no model, service or network connection.

```sh
sealmap review init docs/review/2026-11-03-glm.json \
  --out docs/review/2026-11-03-evidence.json
sealmap review check docs/review/2026-11-03-evidence.json --json
sealmap review summary docs/review/2026-11-03-evidence.json
```

The input is the existing JSON array of findings, each with a unique string
`id`. Other fields stay untouched. `init` binds that file's exact bytes by
SHA-256 and creates one `unverified` disposition per finding. It refuses to
overwrite an existing receipt. Edit the adjacent receipt as evidence arrives;
retain the raw review, commands, logs and corrections in version control or
the established review archive. This is not another issue database.

## What a disposition means

| Status | Required support |
|---|---|
| `unverified` | Original claim, no completed assessment. Missing dispositions also count here. |
| `inconclusive` | Reason the evidence is insufficient; failed setup, unavailable runtime or a wrong probe remain separate observations. |
| `confirmed` | Product failure shown by executed evidence or an exact source trace. Executable reproductions include a positive control. |
| `rejected` | Counterexample or source trace contradicting the claim, recorded as `counterexample` / `expected-behaviour`. Failure to reproduce is insufficient. |
| `known` | An explicit risk acceptance and its decision authority, linked to the decision artefact. This does not mean repaired. |

The denominator is every original finding. Inconclusive observations are never
counted as false positives. `--require-resolved` returns exit 1 while any finding
is unverified or inconclusive; ordinary validation allows honest incompleteness.

A receipt checks evidence *structure and integrity*. A valid receipt does not
establish that an observation is true, that a claimed authority is authorised,
or that the system is secure. Source traces establish their stated source
scope; they are not evidence that a vulnerable path executes in production.

## Recording an observation

After retaining `evidence/replay.log`, a disposition can look like this. Replace
the illustrative hashes, revision and command with the actual captured values:

```json
{
  "finding_id": "area:critical:F-01",
  "status": "confirmed",
  "reason": "The second request reaches the replay assertion in the consumed implementation.",
  "evidence": [{
    "kind": "execution",
    "outcome": "product-failure",
    "captured_at": "2026-11-03T09:00:00Z",
    "repository": "consumer-repository",
    "revision": "FULL_COMMIT_ID",
    "configuration": "default features; isolated fixture; pinned clock",
    "command": "cargo test --locked replay_regression",
    "artifact": { "path": "evidence/replay.log", "sha256": "SHA256_OF_LOG" },
    "observed": "First request succeeds; identical signed request also succeeds.",
    "positive_control": "First request succeeds with the expected durable write."
  }],
  "delivery": []
}
```

Supported outcomes are `product-failure`, `expected-behaviour`, `setup-failure`,
`observation-error`, `inconclusive` and `accepted-risk`. Evidence methods are
`execution`, `source-trace`, `counterexample` and `acceptance`. The last requires
an `authority` string naming the decision and decision-maker. All observations
carry a full revision, capture time, configuration, observed effect and raw
artefact. An execution also carries its command. Recorded commands are never run.

When the tree is dirty, attach `working_tree: {path, sha256}` pointing to the
captured diff or source snapshot; a clean HEAD alone cannot identify those bytes.
For a dependency boundary, include:

```json
{
  "consumer": {
    "name": "pod-worker",
    "version": "the-consumed-version",
    "source": "registry, vendored tree or immutable archive identity",
    "lockfile": { "path": "evidence/Cargo.lock", "sha256": "SHA256_OF_LOCK" }
  }
}
```

The reviewed producer revision and consumed dependency version are distinct.
Use the actual lockfile or packaging receipt, and trace the selected adapter or
feature. Reusing a shared helper is not proof that every consumer calls it.

## Delivery is independent of finding disposition

A confirmed defect remains confirmed after it is repaired. Attach entries to
its `delivery` array with `stage`, `target` and `evidence` (the observation shape
above). Stages are `source`, `tested`, `merged`, `published`, `adopted` and
`deployed`. Every stage needs its own successful evidence; no earlier stage
implies a later one. `tested` and `deployed` require execution evidence.
`adopted` requires a successful observation that itself includes the actual
consumer version and lock artefact. Separate targets can have different stages.

The summary counts attestations per stage, not a completion percentage. A
package can be published while a consumer still uses an older version. A
consumer can be updated in source while the running image still uses old bytes.

## File and failure contract

The evidence root defaults to the receipt's directory; `--evidence-root DIR`
overrides it. All artefact paths are portable relative paths beneath that root.
Parent traversal, absolute paths, symlinks outside the root, missing files and
changed digests fail checks. Keep snapshots beneath the evidence root instead
of reaching into a live sibling checkout. JSON inputs are bounded at 64 MiB;
other artefacts are hashed as streams. This is a local file checker, not a
sandbox against an adversary concurrently replacing the filesystem.

Exit codes: 0 = valid receipt contract; 1 = inconsistent evidence, altered
artefact or unresolved findings under `--require-resolved`; 2 = usage, unreadable
input or malformed JSON. JSON reports include the validity flag, counts, errors
and the semantic-verification limit. Preserve failures instead of replacing
them with a green narrative.

The reusable API is `sealmap_corpus::review`: serde types, `findings`,
`validate`, `artifacts` and `validate_artifact`. Library callers verify artefact
bytes themselves; the CLI supplies filesystem IO and SHA-256. Unknown receipt
fields and unsupported versions are refused. Existing review exports do not
need to migrate to a new findings schema.
