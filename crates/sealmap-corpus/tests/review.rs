//! Evidence dispositions protect the difference between an unavailable check
//! and a disproved finding, and between a source fix and deployment.
use sealmap_corpus::review::{EvidenceFile, findings, validate};
use serde_json::{Value, json};

fn artifact() -> Value {
    json!({"path":"evidence/check.log","sha256":"a".repeat(64)})
}

fn evidence(kind: &str, outcome: &str) -> Value {
    json!({"kind":kind,"outcome":outcome,"captured_at":"2026-11-03T10:00:00Z",
        "repository":"solid-pod-rs","revision":"b".repeat(40),
        "configuration":"default features, local fixture","command":"cargo test regression",
        "artifact":artifact(),"observed":"restricted read denied; owner control succeeds",
        "positive_control":"owner control succeeds"})
}

fn receipt(status: &str, evidence: Vec<Value>) -> Value {
    json!({"version":1,"review":{"path":"review.json","sha256":"c".repeat(64)},
        "dispositions":[{"finding_id":"F-01","status":status,"reason":"examined actual consumer",
            "evidence":evidence,"delivery":[]}]})
}

fn report(value: Value) -> sealmap_corpus::review::Report {
    let file: EvidenceFile = serde_json::from_value(value).unwrap();
    validate(&file, &findings(br#"[{"id":"F-01"},{"id":"F-02"}]"#).unwrap())
}

#[test]
fn setup_failure_cannot_confirm_or_reject_a_finding() {
    let setup = evidence("execution", "setup-failure");
    for status in ["confirmed", "rejected"] {
        assert!(!report(receipt(status, vec![setup.clone()])).valid());
    }
    let report = report(receipt("inconclusive", vec![setup]));
    assert!(report.valid());
    assert_eq!(report.counts["inconclusive"], 1);
    assert_eq!(report.counts["unverified"], 1);
    assert_eq!(report.unresolved, 2);
}

#[test]
fn confirmed_needs_a_product_failure_and_a_positive_control() {
    let positive = evidence("execution", "product-failure");
    assert!(report(receipt("confirmed", vec![positive.clone()])).valid());
    let mut no_control = positive;
    no_control.as_object_mut().unwrap().remove("positive_control");
    assert!(!report(receipt("confirmed", vec![no_control])).valid());
    // A static source trace can confirm only its documented source claim.
    assert!(report(receipt("confirmed", vec![evidence("source-trace", "product-failure")])).valid());
}

#[test]
fn rejected_requires_counterevidence_and_known_requires_acceptance() {
    assert!(!report(receipt("rejected", vec![evidence("execution", "expected-behaviour")])).valid());
    assert!(report(receipt("rejected", vec![evidence("counterexample", "expected-behaviour")])).valid());
    assert!(!report(receipt("known", vec![])).valid());
    let mut accepted = evidence("acceptance", "accepted-risk");
    accepted["authority"] = json!("operator decision recorded in linked artefact");
    assert!(report(receipt("known", vec![accepted])).valid());
}

#[test]
fn missing_ids_duplicates_typos_and_unsupported_versions_fail() {
    let mut value = receipt("unverified", vec![]);
    value["dispositions"][0]["finding_id"] = json!("F-99");
    assert!(!report(value.clone()).valid());
    value["dispositions"][0]["finding_id"] = json!("F-01");
    let duplicate = value["dispositions"][0].clone();
    value["dispositions"].as_array_mut().unwrap().push(duplicate);
    assert!(!report(value).valid());
    let mut wrong_version = receipt("unverified", vec![]);
    wrong_version["version"] = json!(2);
    assert!(!report(wrong_version).valid());
    let mut typo = receipt("unverified", vec![]);
    typo["dispositions"][0]["evidnce"] = json!([]);
    assert!(serde_json::from_value::<EvidenceFile>(typo).is_err());
    assert!(findings(br#"[{"id":"F"},{"id":"F"}]"#).is_err());
    assert!(findings(br#"[{"title":"missing id"}]"#).is_err());
}

#[test]
fn delivery_is_evidenced_separately_never_inferred() {
    let mut value = receipt("confirmed", vec![evidence("execution", "product-failure")]);
    value["dispositions"][0]["delivery"] = json!([
        {"stage":"merged","target":"main","evidence":[evidence("source-trace","expected-behaviour")]}
    ]);
    let merged = report(value.clone());
    assert!(merged.valid());
    assert_eq!(merged.delivery_counts["merged"], 1);
    assert_eq!(merged.delivery_counts["deployed"], 0);
    value["dispositions"][0]["delivery"][0]["stage"] = json!("deployed");
    assert!(!report(value.clone()).valid(), "source trace is not runtime evidence");
    value["dispositions"][0]["delivery"][0]["evidence"] = json!([evidence("execution", "expected-behaviour")]);
    assert!(report(value.clone()).valid());
    value["dispositions"][0]["delivery"][0]["evidence"] = json!([]);
    assert!(!report(value).valid());
}

#[test]
fn consumed_versions_require_the_actual_lock_artifact() {
    let mut item = evidence("source-trace", "product-failure");
    item["consumer"] = json!({"name":"pod-worker","version":"alpha.18","source":"registry","lockfile":artifact()});
    assert!(report(receipt("confirmed", vec![item.clone()])).valid());
    item["consumer"]["lockfile"]["sha256"] = json!("");
    assert!(!report(receipt("confirmed", vec![item])).valid());
}

#[test]
fn failed_consumer_probe_cannot_borrow_an_unrelated_success() {
    let mut value = receipt("confirmed", vec![evidence("source-trace", "product-failure")]);
    let mut failed = evidence("execution", "setup-failure");
    failed["consumer"] = json!({"name":"worker","version":"0.4.0","source":"registry","lockfile":artifact()});
    value["dispositions"][0]["delivery"] = json!([{"stage":"adopted","target":"worker", "evidence":[
        evidence("source-trace", "expected-behaviour"), failed]}]);
    assert!(!report(value.clone()).valid());
    value["dispositions"][0]["delivery"][0]["evidence"][1]["outcome"] = json!("expected-behaviour");
    assert!(report(value).valid());
}

#[test]
fn invalid_revisions_and_path_escapes_fail() {
    let mut item = evidence("execution", "product-failure");
    item["revision"] = json!("HEAD");
    assert!(!report(receipt("confirmed", vec![item.clone()])).valid());
    item["revision"] = json!("d".repeat(40));
    item["artifact"]["path"] = json!("../outside.log");
    assert!(!report(receipt("confirmed", vec![item])).valid());
}
