# Review improvements, 3 November 2026

This work uses build-with-quality and the estate audit's review-evidence lessons.
The operator requested implementation and integration; an Agentbox rebuild may
interrupt it. Progress is checkpointed in shared memory under
`sealmap-modernisation-20261103-active`.

## Expectations

| ID | Behaviour | Counterexample that must fail | Regression critical |
|---|---|---|---|
| EXP-REV-01 | A diff review conservatively selects topics whose source files or relevant configuration changed, including uncited functions and untracked inputs. | Unchanged cited symbol hashes suppress a changed source file. | yes |
| EXP-REV-02 | Review packs identify the bytes and repositories actually read, deterministically. | Two different multi-repository or untracked inputs receive indistinguishable provenance. | yes |
| EXP-REV-03 | Findings retain their raw review; adjacent dispositions distinguish confirmed, rejected, known, inconclusive and unverified. | A setup failure or inability to reproduce becomes a rejection or confirmation. | yes |
| EXP-REV-04 | Dispositions cite exact revisions, configurations, observations and intact evidence artefacts. | An altered log, missing finding, duplicate disposition or unsupported schema passes validation. | yes |
| EXP-REV-05 | Source, tests, merge, publication, consumer adoption and deployment are independent evidence claims. | A merged repair is reported as deployed without deployment evidence. | yes |
| EXP-REV-06 | Agreement requires comparable packs and recognised different model families; it remains a ranking signal. | Different source packs or two aliases for one model count as independent corroboration. | yes |
| EXP-REV-07 | The scheduled workflow retains uncertainty and incomplete results, respects its spend cap and exposes outstanding triage. | Malformed output looks like a clean review; corrupt spending evidence opens a paid gate. | yes |
| EXP-REV-08 | Agentbox consumes the new tooling and instructions through its established package and skill paths. | A new library exists but no supported user journey calls it, or source adoption is described as runtime activation. | yes |

The evidence ledger is an adjacent receipt, not another issue database. It does
not execute supplied commands, certify a model's interpretation, infer authority
from an artefact hash, or silently import old findings as newly reviewed.

Per-file selection remains conservative. Symbol seals remain experimental:
their measured recall was inadequate as a skipping gate. Existing experiments
that found packs no better than full source are retained. New utility claims
must be demonstrated with actual commands, fixtures and exact revisions.

## Acceptance and delivery

Run the workspace's supported default and no-default test configurations,
formatting, clippy and warning-free rustdoc. Test the Agentbox Node adapters
offline and exercise a real CLI receipt journey. Preserve failing regressions
and subsequent checks with their command, output, revision and timestamp.
Perform a separate review of the final diff, naming any unavailable gate or
same-family reviewer limitation. Update the package pin, skills and relevant
ADRs only against the verified release. Record running-image adoption separately;
the operator is rebuilding Agentbox during this work.
