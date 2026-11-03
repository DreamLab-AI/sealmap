//! Evidence-backed dispositions adjacent to an immutable review export.
//!
//! This module checks the shape and consistency of claims. It does **not**
//! decide whether an observation is true, execute a command, or turn a valid
//! receipt into a security certification. Callers must also verify every
//! [`Artifact`] against its bytes; the CLI does this under an evidence root.
//!
//! A failed setup is inconclusive, not counterevidence. Delivery stages are
//! independent attestations: merging source never implies a running fix.
//!
//! ```
//! use sealmap_corpus::review::{EvidenceFile, Artifact, findings, validate};
//! let ids = findings(br#"[{"id":"critical:F-01","status":"unverified"}]"#)?;
//! let file = EvidenceFile {
//!     version: 1,
//!     review: Artifact { path: "review.json".into(), sha256: "a".repeat(64) },
//!     dispositions: vec![],
//! };
//! let report = validate(&file, &ids);
//! assert!(report.valid());
//! assert_eq!(report.unresolved, 1);
//! # Ok::<(), String>(())
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A file relative to the evidence root, identified by SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Portable relative path; no parent traversal or absolute paths.
    pub path: String,
    /// Lower-case hexadecimal SHA-256 of the exact bytes.
    pub sha256: String,
}

/// Versioned adjacent receipt; the original findings are never rewritten.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceFile {
    /// Currently exactly 1.
    pub version: u32,
    /// Original JSON array of findings, each with a unique string `id`.
    pub review: Artifact,
    /// Assessments for some or all original findings. Missing means unverified.
    pub dispositions: Vec<Disposition>,
}

/// Human or agent assessment, separate from the original reviewer's claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disposition {
    /// Exact id from the original review export.
    pub finding_id: String,
    /// What the evidence supports about the claim, not its delivery state.
    pub status: Status,
    /// Explanation, required for every assessment except unverified.
    pub reason: String,
    /// Observations used for this assessment.
    pub evidence: Vec<Evidence>,
    /// Independently evidenced stages of a repair; absence means unknown.
    pub delivery: Vec<Delivery>,
}

/// Finding dispositions. Inconclusive is never counted as a false positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Imported claim without a completed assessment.
    Unverified,
    /// Evidence demonstrates the stated defect in its recorded scope.
    Confirmed,
    /// Counterevidence contradicts the stated claim.
    Rejected,
    /// Explicitly acknowledged and accepted risk, with an authority reference.
    Known,
    /// Evidence or runtime access is insufficient for a verdict.
    Inconclusive,
}

impl Status {
    /// Stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Known => "known",
            Self::Inconclusive => "inconclusive",
        }
    }
}

/// How an observation was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceKind {
    /// Executed command and captured output.
    Execution,
    /// Exact source trace; makes no claim to runtime execution.
    SourceTrace,
    /// Counterexample or source trace that contradicts the claim.
    Counterexample,
    /// Recorded risk decision, not proof that a defect is fixed.
    Acceptance,
}

/// What the observation actually established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// The product violated the recorded contract.
    ProductFailure,
    /// The scoped contract held in this observation.
    ExpectedBehaviour,
    /// A prerequisite failed before the intended assertion.
    SetupFailure,
    /// The probe or interpretation was wrong.
    ObservationError,
    /// No supported conclusion.
    Inconclusive,
    /// An authority accepted the recorded risk.
    AcceptedRisk,
}

/// A version as actually consumed, distinct from the producer's current HEAD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consumer {
    /// Consumer component or repository.
    pub name: String,
    /// Consumed version or full revision.
    pub version: String,
    /// Registry, vendored source or archive identity.
    pub source: String,
    /// Exact lockfile or packaging receipt that establishes consumption.
    pub lockfile: Artifact,
}

/// One bounded observation. Commands are recorded, never executed by this module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Observation method.
    pub kind: EvidenceKind,
    /// Result reached, rather than the intended assertion.
    pub outcome: Outcome,
    /// Capture timestamp including timezone (recorded, not used as a freshness oracle).
    pub captured_at: String,
    /// Repository or component identifier, not automatically dereferenced.
    pub repository: String,
    /// Full Git commit id (40 or 64 lower-case hexadecimal digits).
    pub revision: String,
    /// Features, fixture, environment and other relevant scope limits.
    pub configuration: String,
    /// Command that produced an execution observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Raw output, source trace or decision record.
    pub artifact: Artifact,
    /// State or effect actually observed, including the reached assertion.
    pub observed: String,
    /// Successful control for an executable defect reproduction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positive_control: Option<String>,
    /// Decision-maker and decision reference for accepted risk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    /// Exact source/diff snapshot when the examined checkout was dirty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_tree: Option<Artifact>,
    /// Optional consumer boundary; a producer revision alone cannot fill this in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer: Option<Consumer>,
}

/// Repair stage; no ordering or implication between stages is assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Implementation exists in the recorded source.
    Source,
    /// Relevant tests executed successfully.
    Tested,
    /// Merged into the named target branch.
    Merged,
    /// Released artefact exists at its named destination.
    Published,
    /// Named consumer actually resolves the released implementation.
    Adopted,
    /// Named runtime was observed running the repair.
    Deployed,
}

impl Stage {
    /// Stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Tested => "tested",
            Self::Merged => "merged",
            Self::Published => "published",
            Self::Adopted => "adopted",
            Self::Deployed => "deployed",
        }
    }
}

/// Evidence for a single repair target and stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    /// Stage demonstrated, without implying any later stage.
    pub stage: Stage,
    /// Branch, registry artefact, consumer or running deployment.
    pub target: String,
    /// Evidence of this stage rather than of the original defect.
    pub evidence: Vec<Evidence>,
}

/// Deterministic report; counts do not imply semantic verification.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Number of original findings, including those without dispositions.
    pub findings_total: usize,
    /// Counts by assessment status; includes zero-valued categories.
    pub counts: BTreeMap<String, usize>,
    /// Unverified plus inconclusive, with the original denominator preserved.
    pub unresolved: usize,
    /// Number of independent stage attestations; not findings or progress percentage.
    pub delivery_counts: BTreeMap<String, usize>,
    /// All structural/evidence problems, in deterministic input order.
    pub errors: Vec<String>,
}

impl Report {
    /// Whether the receipt contract is satisfied; not whether the system is correct.
    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Read original finding ids without changing or interpreting their other fields.
pub fn findings(bytes: &[u8]) -> Result<BTreeSet<String>, String> {
    let rows: Vec<serde_json::Value> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| format!("finding {index}: non-empty string id required"))?;
        if !ids.insert(id.to_owned()) {
            return Err(format!("duplicate finding id `{id}`"));
        }
    }
    Ok(ids)
}

fn hex(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len()) && value.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn nonempty(value: &str, name: &str, errors: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{name}: must not be empty"));
    }
}

/// Validate a portable artefact reference, without reading the filesystem.
pub fn validate_artifact(a: &Artifact) -> Result<(), String> {
    if a.path.is_empty()
        || a.path.contains(['\\', ':', '\0'])
        || a.path.split('/').any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(format!("artefact path `{}` must be a portable relative file path", a.path));
    }
    if !hex(&a.sha256, &[64]) {
        return Err(format!("{}: expected lower-case SHA-256", a.path));
    }
    Ok(())
}

fn check_evidence(e: &Evidence, errors: &mut Vec<String>) {
    for (name, value) in [
        ("captured_at", &e.captured_at),
        ("repository", &e.repository),
        ("configuration", &e.configuration),
        ("observed", &e.observed),
    ] {
        nonempty(value, name, errors);
    }
    if !hex(&e.revision, &[40, 64]) {
        errors.push("revision: full lower-case Git commit required".into());
    }
    if e.kind == EvidenceKind::Execution && e.command.as_deref().unwrap_or("").trim().is_empty() {
        errors.push("execution evidence requires the executed command".into());
    }
    if e.kind == EvidenceKind::Execution
        && e.outcome == Outcome::ProductFailure
        && e.positive_control.as_deref().unwrap_or("").trim().is_empty()
    {
        errors.push("executable defect reproduction requires a positive control".into());
    }
    if e.kind == EvidenceKind::Acceptance
        && (e.outcome != Outcome::AcceptedRisk || e.authority.as_deref().unwrap_or("").trim().is_empty())
    {
        errors.push("acceptance requires accepted-risk outcome and decision authority".into());
    }
    if let Some(c) = &e.consumer {
        for (name, value) in
            [("consumer.name", &c.name), ("consumer.version", &c.version), ("consumer.source", &c.source)]
        {
            nonempty(value, name, errors);
        }
    }
}

/// Every referenced artefact, including original review, dirty snapshots and locks.
/// Repeated references are retained so conflicting digests cannot be hidden.
pub fn artifacts(file: &EvidenceFile) -> Vec<&Artifact> {
    let mut out = vec![&file.review];
    for disposition in &file.dispositions {
        for e in disposition.evidence.iter().chain(disposition.delivery.iter().flat_map(|d| &d.evidence)) {
            out.push(&e.artifact);
            if let Some(snapshot) = &e.working_tree {
                out.push(snapshot);
            }
            if let Some(consumer) = &e.consumer {
                out.push(&consumer.lockfile);
            }
        }
    }
    out
}

/// Check assessments against the original ids. Hash verification is a separate IO step.
pub fn validate(file: &EvidenceFile, ids: &BTreeSet<String>) -> Report {
    let mut report = Report {
        findings_total: ids.len(),
        counts: BTreeMap::from([
            ("unverified".into(), ids.len()),
            ("confirmed".into(), 0),
            ("rejected".into(), 0),
            ("known".into(), 0),
            ("inconclusive".into(), 0),
        ]),
        unresolved: ids.len(),
        delivery_counts: ["source", "tested", "merged", "published", "adopted", "deployed"]
            .into_iter()
            .map(|s| (s.into(), 0))
            .collect(),
        errors: Vec::new(),
    };
    if file.version != 1 {
        report.errors.push(format!("unsupported evidence version {} (expected 1)", file.version));
    }
    for artifact in artifacts(file) {
        if let Err(error) = validate_artifact(artifact) {
            report.errors.push(error);
        }
    }
    let mut seen = BTreeSet::new();
    for d in &file.dispositions {
        let start = report.errors.len();
        if !ids.contains(&d.finding_id) {
            report.errors.push("finding absent from original review".into());
        } else if !seen.insert(&d.finding_id) {
            report.errors.push("duplicate disposition".into());
        } else {
            *report.counts.get_mut("unverified").expect("initialised") -= 1;
            *report.counts.get_mut(d.status.as_str()).expect("all statuses initialised") += 1;
        }
        if d.status != Status::Unverified {
            nonempty(&d.reason, "reason", &mut report.errors);
        }
        for e in &d.evidence {
            check_evidence(e, &mut report.errors);
        }
        let supported = match d.status {
            Status::Unverified | Status::Inconclusive => true,
            Status::Confirmed => d.evidence.iter().any(|e| {
                matches!(e.kind, EvidenceKind::Execution | EvidenceKind::SourceTrace)
                    && e.outcome == Outcome::ProductFailure
            }),
            Status::Rejected => d
                .evidence
                .iter()
                .any(|e| e.kind == EvidenceKind::Counterexample && e.outcome == Outcome::ExpectedBehaviour),
            Status::Known => {
                d.evidence.iter().any(|e| e.kind == EvidenceKind::Acceptance && e.outcome == Outcome::AcceptedRisk)
            }
        };
        if !supported {
            report.errors.push(format!("{} disposition lacks supporting evidence", d.status.as_str()));
        }
        let mut targets = BTreeSet::new();
        for delivery in &d.delivery {
            if d.status != Status::Confirmed {
                report.errors.push("repair delivery requires a confirmed finding".into());
            }
            nonempty(&delivery.target, "delivery.target", &mut report.errors);
            if !targets.insert((delivery.stage, &delivery.target)) {
                report.errors.push("duplicate stage/target attestation".into());
            }
            let runtime = matches!(delivery.stage, Stage::Tested | Stage::Deployed);
            if !delivery.evidence.iter().any(|e| {
                e.outcome == Outcome::ExpectedBehaviour
                    && if runtime {
                        e.kind == EvidenceKind::Execution
                    } else {
                        matches!(e.kind, EvidenceKind::Execution | EvidenceKind::SourceTrace)
                    }
            }) {
                report.errors.push(format!(
                    "{} requires its own successful {}evidence",
                    delivery.stage.as_str(),
                    if runtime { "execution " } else { "" }
                ));
            }
            if delivery.stage == Stage::Adopted
                && !delivery.evidence.iter().any(|e| {
                    e.consumer.is_some()
                        && e.outcome == Outcome::ExpectedBehaviour
                        && matches!(e.kind, EvidenceKind::Execution | EvidenceKind::SourceTrace)
                })
            {
                report
                    .errors
                    .push("adopted requires successful evidence of the consumed version and lock artefact".into());
            }
            for e in &delivery.evidence {
                check_evidence(e, &mut report.errors);
            }
            *report.delivery_counts.get_mut(delivery.stage.as_str()).expect("all stages initialised") += 1;
        }
        for error in &mut report.errors[start..] {
            *error = format!("{}: {error}", d.finding_id);
        }
    }
    report.unresolved = report.counts["unverified"] + report.counts["inconclusive"];
    report
}
