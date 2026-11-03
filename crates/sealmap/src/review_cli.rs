//! Adjacent evidence receipts; no supplied command is ever executed.
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};
use sealmap::corpus::review::{self, Artifact, Disposition, EvidenceFile, Status};
use sha2::{Digest, Sha256};

#[derive(Subcommand)]
pub(crate) enum Review {
    /// Create adjacent unverified dispositions without modifying original findings.
    Init(Init),
    /// Validate receipt contracts and every referenced artefact's bytes.
    Check(Check),
    /// Validate and summarise assessments and independently evidenced delivery stages.
    Summary(Check),
}

#[derive(Args)]
pub(crate) struct Init {
    /// Original JSON array of findings with unique ids.
    findings: PathBuf,
    /// New receipt file; existing files are never overwritten.
    #[arg(short, long)]
    out: PathBuf,
    /// Root for portable artefact paths (default: receipt file's parent).
    #[arg(long)]
    evidence_root: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct Check {
    /// Version-1 evidence receipt JSON.
    receipts: PathBuf,
    /// Root for artefact paths (default: receipt file's parent).
    #[arg(long)]
    evidence_root: Option<PathBuf>,
    /// Machine-readable report, including all errors.
    #[arg(long)]
    json: bool,
    /// Exit 1 while any original finding remains unverified or inconclusive.
    #[arg(long)]
    require_resolved: bool,
}

fn parent(path: &Path) -> &Path {
    path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_json(path: &Path) -> Result<Vec<u8>, String> {
    const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
    let f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut data = Vec::new();
    f.take(MAX_JSON_BYTES + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_JSON_BYTES {
        return Err(format!("{}: JSON exceeds 64 MiB", path.display()));
    }
    Ok(data)
}

fn digest(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = f.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn confined(root: &Path, artifact: &Artifact) -> Result<PathBuf, String> {
    review::validate_artifact(artifact)?;
    let path = canonical(&root.join(&artifact.path))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(format!("{}: artefact is not a regular file within evidence root", artifact.path));
    }
    Ok(path)
}

fn init(args: Init) -> Result<ExitCode, String> {
    let root = canonical(args.evidence_root.as_deref().unwrap_or_else(|| parent(&args.out)))?;
    let source = canonical(&args.findings)?;
    let relative = source.strip_prefix(&root).map_err(|_| "findings must be inside evidence root")?;
    let path = relative.to_str().ok_or("artefact path must be UTF-8")?.replace(std::path::MAIN_SEPARATOR, "/");
    let bytes = read_json(&source)?;
    let review = Artifact { path, sha256: format!("{:x}", Sha256::digest(&bytes)) };
    review::validate_artifact(&review)?;
    let ids = review::findings(&bytes)?;
    let file = EvidenceFile {
        version: 1,
        review,
        dispositions: ids
            .into_iter()
            .map(|id| Disposition {
                finding_id: id,
                status: Status::Unverified,
                reason: String::new(),
                evidence: vec![],
                delivery: vec![],
            })
            .collect(),
    };
    let data = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())? + "\n";
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .map_err(|e| format!("{}: {e}", args.out.display()))?;
    if let Err(error) = out.write_all(data.as_bytes()).and_then(|()| out.sync_all()) {
        drop(out);
        let _ = fs::remove_file(&args.out);
        return Err(format!("{}: {error}", args.out.display()));
    }
    eprintln!("sealmap: {} unverified findings -> {}", file.dispositions.len(), args.out.display());
    Ok(ExitCode::SUCCESS)
}

fn check(args: Check) -> Result<ExitCode, String> {
    let file: EvidenceFile = serde_json::from_slice(&read_json(&args.receipts)?).map_err(|e| e.to_string())?;
    let root = canonical(args.evidence_root.as_deref().unwrap_or_else(|| parent(&args.receipts)))?;
    let original = confined(&root, &file.review)?;
    let bytes = read_json(&original)?;
    let ids = review::findings(&bytes)?;
    let mut report = review::validate(&file, &ids);
    if format!("{:x}", Sha256::digest(&bytes)) != file.review.sha256 {
        report.errors.push("original findings bytes do not match the recorded SHA-256".into());
    }
    for artifact in review::artifacts(&file) {
        let result = confined(&root, artifact).and_then(|path| digest(&path)).and_then(|actual| {
            if actual == artifact.sha256 {
                Ok(())
            } else {
                Err(format!("{}: SHA-256 mismatch (expected {}, got {actual})", artifact.path, artifact.sha256))
            }
        });
        if let Err(error) = result {
            report.errors.push(error);
        }
    }
    let valid = report.valid();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "schema_version":1,"valid":valid,"report":report,
            "boundary":"Receipt consistency and artefact integrity only; observations and authority are not independently verified."
        })).map_err(|e| e.to_string())?);
    } else {
        println!("{} findings; {} unresolved", report.findings_total, report.unresolved);
        for (status, count) in &report.counts {
            println!("{status}: {count}");
        }
        for (stage, count) in &report.delivery_counts {
            println!("delivery {stage}: {count} attestation(s)");
        }
        for error in &report.errors {
            println!("ERROR: {error}");
        }
        println!(
            "Receipt consistency and artefact integrity only; observations and authority are not independently verified."
        );
    }
    Ok(if valid && (!args.require_resolved || report.unresolved == 0) { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

pub(crate) fn run(command: Review) -> Result<ExitCode, String> {
    match command {
        Review::Init(args) => init(args),
        Review::Check(args) | Review::Summary(args) => check(args),
    }
}
