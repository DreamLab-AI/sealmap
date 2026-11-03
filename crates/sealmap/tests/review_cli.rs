//! Complete evidence workflow and tamper/failure controls through the shipped CLI.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("review-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("raw.json"), br#"[{"id":"F-1","status":"unverified","body":"original claim"}]"#).unwrap();
    dir
}

fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sealmap")).current_dir(dir).args(args).output().unwrap()
}

fn init(dir: &Path) {
    let output = cli(dir, &["review", "init", "raw.json", "--out", "receipt.json"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn save(dir: &Path, value: &Value) {
    fs::write(dir.join("receipt.json"), serde_json::to_vec(value).unwrap()).unwrap();
}

fn receipt(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("receipt.json")).unwrap()).unwrap()
}

fn evidence(dir: &Path) -> Value {
    let log = "positive control passed; replay assertion failed at expected assertion\n";
    fs::write(dir.join("check.log"), log).unwrap();
    json!({"kind":"execution","outcome":"product-failure","captured_at":"2026-11-03T11:00:00Z",
        "repository":"fixture","revision":"a".repeat(40),"configuration":"default features, fixture",
        "command":"cargo test replay","artifact":{"path":"check.log","sha256":format!("{:x}",Sha256::digest(log.as_bytes()))},
        "observed":"replay accepted","positive_control":"initial request succeeds"})
}

#[test]
fn original_immutable_initialisation_and_summary_keep_unknowns() {
    let dir = scratch("journey");
    let original = fs::read(dir.join("raw.json")).unwrap();
    init(&dir);
    let output = cli(&dir, &["review", "summary", "receipt.json", "--json"]);
    assert!(output.status.success());
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["report"]["counts"]["unverified"], 1);
    assert_eq!(summary["report"]["delivery_counts"]["deployed"], 0);
    assert_eq!(cli(&dir, &["review", "check", "receipt.json", "--require-resolved"]).status.code(), Some(1));
    let before = fs::read(dir.join("receipt.json")).unwrap();
    assert_eq!(cli(&dir, &["review", "init", "raw.json", "--out", "receipt.json"]).status.code(), Some(2));
    assert_eq!(fs::read(dir.join("receipt.json")).unwrap(), before);
    assert_eq!(fs::read(dir.join("raw.json")).unwrap(), original);
}

#[test]
fn confirmed_receipt_passes_but_tampered_log_or_original_does_not() {
    let dir = scratch("tamper");
    init(&dir);
    let mut r = receipt(&dir);
    r["dispositions"][0]["status"] = json!("confirmed");
    r["dispositions"][0]["reason"] = json!("reproduced actual consumer");
    r["dispositions"][0]["evidence"] = json!([evidence(&dir)]);
    save(&dir, &r);
    let output = cli(&dir, &["review", "check", "receipt.json", "--require-resolved"]);
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(dir.join("check.log"), "different result").unwrap();
    assert_eq!(cli(&dir, &["review", "check", "receipt.json"]).status.code(), Some(1));
    evidence(&dir);
    fs::write(dir.join("raw.json"), br#"[{"id":"F-1","body":"different claim"}]"#).unwrap();
    assert_eq!(cli(&dir, &["review", "check", "receipt.json"]).status.code(), Some(1));
}

#[test]
fn recorded_commands_never_execute_and_blocked_probe_stays_unresolved() {
    let dir = scratch("setup");
    init(&dir);
    let mut r = receipt(&dir);
    let mut e = evidence(&dir);
    e["outcome"] = json!("setup-failure");
    e["command"] = json!("touch MUST-NOT-EXIST");
    r["dispositions"][0]["status"] = json!("inconclusive");
    r["dispositions"][0]["reason"] = json!("fixture failed before replay check");
    r["dispositions"][0]["evidence"] = json!([e]);
    save(&dir, &r);
    assert!(cli(&dir, &["review", "check", "receipt.json"]).status.success());
    assert!(!dir.join("MUST-NOT-EXIST").exists());
    r["dispositions"][0]["status"] = json!("rejected");
    save(&dir, &r);
    assert_eq!(cli(&dir, &["review", "check", "receipt.json"]).status.code(), Some(1));
}

#[cfg(unix)]
#[test]
fn artifact_symlinks_cannot_read_outside_evidence_root() {
    use std::os::unix::fs::symlink;
    let dir = scratch("symlink");
    init(&dir);
    let mut r = receipt(&dir);
    let mut e = evidence(&dir);
    let outside = scratch("outside");
    fs::copy(dir.join("check.log"), outside.join("outside.log")).unwrap();
    symlink(outside.join("outside.log"), dir.join("link.log")).unwrap();
    e["artifact"]["path"] = json!("link.log");
    r["dispositions"][0]["evidence"] = json!([e]);
    save(&dir, &r);
    assert_eq!(cli(&dir, &["review", "check", "receipt.json"]).status.code(), Some(1));
}
