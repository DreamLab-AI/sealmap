//! The `sealmap` binary end to end on a scratch repository: sign, verify,
//! seal-check, resolve, stale (with and without `--since`) and
//! `generate --check`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ACCOUNTS: &str = "pub struct LedgerAccount { pub balance: i64 }\n\nimpl LedgerAccount {\n    pub fn deposit(&mut self, amount: i64) {\n        self.balance += amount;\n    }\n\n    pub fn withdraw(&mut self, amount: i64) -> bool {\n        self.balance -= amount;\n        self.balance >= 0\n    }\n}\n";

const TOPIC: &str = "---\nid: LED-01\ntitle: Accounts\narea: ledger\n---\nMoney goes in through `sym:cargo ledger . accounts/LedgerAccount#deposit().` and out\nthrough `sym:cargo ledger . accounts/LedgerAccount#withdraw().`.\n";

const DEPOSIT: &str = "sym:cargo ledger . accounts/LedgerAccount#deposit().";

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("docs/diagrams/ledger")).unwrap();
    fs::write(dir.join("Cargo.toml"), "[package]\nname = \"ledger\"\n").unwrap();
    fs::write(dir.join("src/lib.rs"), "pub mod accounts;\n").unwrap();
    fs::write(dir.join("src/accounts.rs"), ACCOUNTS).unwrap();
    fs::write(dir.join("docs/diagrams/ledger/01-accounts.md"), TOPIC).unwrap();
    dir
}

/// Stops git searching above the scratch root. Without it, a target directory
/// inside a checkout (as on CI) makes every fixture part of that checkout.
const GIT_CEILING: &str = env!("CARGO_TARGET_TMPDIR");

fn sealmap(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sealmap"))
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", GIT_CEILING)
        .args(args)
        .output()
        .unwrap()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn edit(dir: &Path, file: &str, from: &str, to: &str) {
    let p = dir.join(file);
    let t = fs::read_to_string(&p).unwrap();
    assert!(t.contains(from), "{from}");
    fs::write(p, t.replace(from, to)).unwrap();
}

fn sign(dir: &Path) {
    let o = sealmap(
        dir,
        &[
            "seal",
            "sign",
            "LED-01",
            "--reviewer",
            "zai:glm-5.3",
            "--model",
            "claude:claude-sonnet-5-5",
            "--date",
            "2026-10-05",
        ],
    );
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn sign_then_verify_goes_red_after_a_body_edit() {
    let dir = scratch("sign-verify");
    // Unsealed citations fail the gate before signing.
    assert_eq!(code(&sealmap(&dir, &["verify"])), 1);
    sign(&dir);
    let lock = fs::read_to_string(dir.join("docs/diagrams/seals.lock")).unwrap();
    assert!(lock.contains("id = \"LED-01\"\nfile = \"ledger/01-accounts.md\""), "{lock}");
    assert!(lock.contains("reviewer = \"zai:glm-5.3\""));
    let topic = fs::read_to_string(dir.join("docs/diagrams/ledger/01-accounts.md")).unwrap();
    assert!(topic.contains("area: ledger\nsealed: seals.lock\n---"), "{topic}");

    assert_eq!(code(&sealmap(&dir, &["verify"])), 0);
    assert_eq!(code(&sealmap(&dir, &["seal-check", "LED-01"])), 0);
    assert_eq!(code(&sealmap(&dir, &["seal-check", "LED-09"])), 1);
    // Signing again is idempotent.
    sign(&dir);
    assert_eq!(fs::read_to_string(dir.join("docs/diagrams/seals.lock")).unwrap(), lock);

    edit(&dir, "src/accounts.rs", "self.balance += amount;", "self.balance += amount * 2;");
    let o = sealmap(&dir, &["verify"]);
    assert_eq!(code(&o), 1);
    assert!(stdout(&o).contains(&format!("behaviour         LED-01   {DEPOSIT}")), "{}", stdout(&o));
    let o = sealmap(&dir, &["verify", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v["findings"].as_array().unwrap().iter().any(|f| f["class"] == "behaviour" && f["symbol"] == DEPOSIT));

    // stale reports it and still exits 0.
    let o = sealmap(&dir, &["stale"]);
    assert_eq!(code(&o), 0);
    assert!(stdout(&o).contains(&format!("LED-01   behaviour  {DEPOSIT}")), "{}", stdout(&o));

    // A lock that is not canonical is a fault even when every seal holds.
    edit(&dir, "src/accounts.rs", "self.balance += amount * 2;", "self.balance += amount;");
    assert_eq!(code(&sealmap(&dir, &["verify"])), 0);
    fs::write(dir.join("docs/diagrams/seals.lock"), format!("{lock}\n")).unwrap();
    let o = sealmap(&dir, &["verify"]);
    assert_eq!(code(&o), 1);
    assert!(stdout(&o).contains("lock-fault"), "{}", stdout(&o));
}

#[test]
fn resolve_prints_span_and_hashes_or_absent() {
    let dir = scratch("resolve");
    let o = sealmap(&dir, &["resolve", DEPOSIT]);
    assert_eq!(code(&o), 0);
    assert!(stdout(&o).starts_with("found src/accounts.rs:4-6\nsig   blake3-16:"), "{}", stdout(&o));
    let o = sealmap(&dir, &["resolve", "--json", DEPOSIT]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["resolution"]["status"], "found");
    assert_eq!(v["resolution"]["span"]["start_line"], 4);

    // After a rename, the lock's sealed body names the candidate.
    sign(&dir);
    edit(&dir, "src/accounts.rs", "fn deposit(", "fn credit(");
    let o = sealmap(&dir, &["resolve", DEPOSIT]);
    assert_eq!(code(&o), 1);
    assert_eq!(stdout(&o), "absent\ncandidate sym:cargo ledger . accounts/LedgerAccount#credit().\n");
    assert_eq!(code(&sealmap(&dir, &["resolve", "not an id"])), 2);
}

#[test]
fn stale_since_a_git_revision_compares_two_trees() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git not available; skipping");
        return;
    }
    let dir = scratch("stale-since");
    sign(&dir);
    // Change deposit, then commit: that change predates the revision.
    edit(&dir, "src/accounts.rs", "self.balance += amount;", "self.balance += amount + 0;");
    let git = |args: &[&str]| {
        let o = Command::new("git")
            .current_dir(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "base"]);
    // Then change withdraw's signature in the working tree.
    edit(&dir, "src/accounts.rs", "-> bool {", "-> i64 {");
    edit(&dir, "src/accounts.rs", "self.balance >= 0", "self.balance");

    let since = stdout(&sealmap(&dir, &["stale", "--since", "HEAD"]));
    assert!(since.contains("contract   sym:cargo ledger . accounts/LedgerAccount#withdraw()."), "{since}");
    assert!(!since.contains(DEPOSIT), "{since}");
    let against_lock = stdout(&sealmap(&dir, &["stale"]));
    assert!(against_lock.contains(DEPOSIT) && against_lock.contains("withdraw"), "{against_lock}");
    // The temporary tree is cleaned up and the checkout is untouched.
    assert_eq!(code(&sealmap(&dir, &["stale", "--since", "no-such-rev"])), 2);
}

#[test]
fn generate_check_compares_with_a_fresh_generation() {
    let dir = scratch("generate-check");
    assert_eq!(code(&sealmap(&dir, &["generate", "--check"])), 1);
    assert!(!dir.join(".sealmap").exists(), "--check writes nothing");
    assert_eq!(code(&sealmap(&dir, &["generate"])), 0);
    assert!(dir.join(".sealmap/src/accounts.rs.md").exists());
    assert_eq!(code(&sealmap(&dir, &["generate", "--check"])), 0);
    edit(&dir, "src/accounts.rs", "self.balance += amount;", "self.balance += amount * 2;");
    let o = sealmap(&dir, &["generate", "--check"]);
    assert_eq!(code(&o), 1);
    assert!(stdout(&o).contains("stale     src/accounts.rs.md"), "{}", stdout(&o));
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", GIT_CEILING)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

fn has_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// A second topic citing a symbol of its own, so packs and `--diff` have a
/// choice to make.
const TOPIC2: &str = "---\nid: LED-02\ntitle: Withdrawals\narea: ledger\n---\nOverdrafts are possible: `sym:cargo ledger . accounts/LedgerAccount#withdraw().`.\n";

fn scratch2(name: &str) -> PathBuf {
    let dir = scratch(name);
    fs::write(dir.join("docs/diagrams/ledger/02-withdrawals.md"), TOPIC2).unwrap();
    dir
}

#[test]
fn pack_is_byte_identical_across_runs_and_argument_order() {
    let dir = scratch2("pack-determinism");
    let a = sealmap(&dir, &["pack", "LED-01", "LED-02"]);
    assert_eq!(code(&a), 0, "{}", String::from_utf8_lossy(&a.stderr));
    let b = sealmap(&dir, &["pack", "ledger/02-withdrawals.md", "LED-01", "LED-02"]);
    assert_eq!(a.stdout, b.stdout);
    let text = stdout(&a);
    assert!(text.starts_with("# sealmap-pack 2\n"), "{text}");
    assert!(
        text.contains("\nrevision: unknown\ntopics: LED-01 LED-02\nbudget: none\ndepth: 1\nsource-window: 40 lines\n")
    );
    assert!(text.contains(&format!("==== source LED-01 src/accounts.rs:L4-6 83 bytes {DEPOSIT}\n")), "{text}");
    // To a file: the same bytes.
    assert_eq!(code(&sealmap(&dir, &["pack", "LED-02", "LED-01", "-o", "pack.txt"])), 0);
    assert_eq!(fs::read(dir.join("pack.txt")).unwrap(), a.stdout);
    // Nothing to pack is a usage error.
    assert_eq!(code(&sealmap(&dir, &["pack"])), 2);
    assert_eq!(code(&sealmap(&dir, &["pack", "LED-09"])), 2);
}

#[test]
fn pack_over_budget_refuses_with_sizes_and_shards_on_request() {
    let dir = scratch2("pack-budget");
    let full = stdout(&sealmap(&dir, &["pack", "LED-01", "LED-02", "--budget", "9999"]));
    // The budget is part of the header: keep its width (four digits) the same.
    assert!((1001..=9999).contains(&full.len()), "{}", full.len());
    let budget = (full.len() - 1).to_string();
    let o = sealmap(&dir, &["pack", "LED-01", "LED-02", "--budget", &budget]);
    assert_eq!(code(&o), 1);
    assert!(o.stdout.is_empty(), "nothing is written when refusing");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("1 over the") && err.contains("refusing rather than truncating"), "{err}");
    assert!(err.contains("\nLED-01 ") && err.contains("\nLED-02 ") && err.contains("\nheader "), "{err}");
    let o = sealmap(&dir, &["pack", "LED-01", "LED-02", "--budget", &budget, "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["error"], "over_budget");
    assert_eq!(v["topics"].as_array().unwrap().len(), 2);

    // --shard splits by topic into numbered files, each within the budget.
    let o = sealmap(&dir, &["pack", "LED-01", "LED-02", "--budget", &budget, "--shard", "-o", "packs"]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let one = fs::read_to_string(dir.join("packs/pack-01.txt")).unwrap();
    let two = fs::read_to_string(dir.join("packs/pack-02.txt")).unwrap();
    assert!(one.contains("\ntopics: LED-01\n") && one.contains("\nshard: 1\n"), "{one}");
    assert!(two.contains("\ntopics: LED-02\n") && two.contains("\nshard: 2\n"), "{two}");
    assert!(one.len() < full.len() && two.len() < full.len());
    // --shard needs a budget and a directory.
    assert_eq!(code(&sealmap(&dir, &["pack", "LED-01", "--shard"])), 2);
}

#[test]
fn pack_diff_selects_topics_whose_cited_files_changed_since_a_revision() {
    if !has_git() {
        eprintln!("git not available; skipping");
        return;
    }
    let dir = scratch2("pack-diff");
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    let head =
        String::from_utf8(Command::new("git").current_dir(&dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout)
            .unwrap();

    // Nothing changed: nothing to pack, and that is not an error.
    let o = sealmap(&dir, &["pack", "--diff", "HEAD"]);
    assert_eq!(code(&o), 0);
    assert!(o.stdout.is_empty());

    // Change deposit only: both topics cite the same source file.
    edit(&dir, "src/accounts.rs", "self.balance += amount;", "self.balance += amount * 2;");
    let o = sealmap(&dir, &["pack", "--diff", "HEAD"]);
    assert_eq!(code(&o), 0, "{}", String::from_utf8_lossy(&o.stderr));
    let text = stdout(&o);
    assert!(
        text.contains(&format!("\nrevision: {}+dirty\ndiff: HEAD\ntopics: LED-01 LED-02\n", head.trim())),
        "{text}"
    );
    assert!(text.contains("self.balance += amount * 2;"), "the window shows the current code: {text}");
    // Explicit topics join the changed ones.
    let both = stdout(&sealmap(&dir, &["pack", "LED-02", "--diff", "HEAD"]));
    assert!(both.contains("\ntopics: LED-01 LED-02\n"), "{both}");

    // Committed, the change is behind HEAD but not behind HEAD~0's parent.
    git(&dir, &["commit", "-q", "-am", "double deposits"]);
    assert!(stdout(&sealmap(&dir, &["pack", "--diff", "HEAD"])).is_empty());
    let since_base = stdout(&sealmap(&dir, &["pack", "--diff", head.trim()]));
    assert!(since_base.contains("\ntopics: LED-01 LED-02\n") && !since_base.contains("+dirty"), "{since_base}");
    assert_eq!(code(&sealmap(&dir, &["pack", "--diff", "no-such-rev"])), 2);
}

#[test]
fn pack_diff_keeps_same_file_topics_and_global_dependency_changes() {
    if !has_git() {
        return;
    }
    let dir = scratch2("pack-file-baseline");
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    // The uncited caller/context can change the documented contract even when
    // a cited symbol's body is identical. File-level selection keeps both.
    edit(&dir, "src/accounts.rs", "self.balance += amount;", "self.balance += amount * 2;");
    let out = sealmap(&dir, &["pack", "--diff", "HEAD"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("\ntopics: LED-01 LED-02\n"), "{}", stdout(&out));
    git(&dir, &["commit", "-q", "-am", "changed implementation"]);
    fs::write(dir.join("Cargo.lock"), "version = 4\n").unwrap();
    let out = sealmap(&dir, &["pack", "--diff", "HEAD"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("\ntopics: LED-01 LED-02\n"), "{}", stdout(&out));
}

#[test]
fn pack_provenance_binds_untracked_inputs_but_ignores_its_output() {
    if !has_git() {
        return;
    }
    let dir = scratch("pack-provenance");
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    let clean = stdout(&sealmap(&dir, &["pack", "LED-01"]));
    fs::write(dir.join("src/new.rs"), "pub fn untracked() {}\n").unwrap();
    let untracked = stdout(&sealmap(&dir, &["pack", "LED-01"]));
    assert_ne!(clean, untracked, "untracked extraction inputs must be bound");
    assert!(untracked.contains("source-inputs: blake3:"));
    assert!(untracked.contains("src/new.rs"), "untracked input is visible in provenance");
    fs::write(dir.join("review-pack.txt"), &untracked).unwrap();
    assert_eq!(untracked, stdout(&sealmap(&dir, &["pack", "LED-01"])));
    fs::write(dir.join("Cargo.lock"), "version = 4\n").unwrap();
    let with_lock = stdout(&sealmap(&dir, &["pack", "LED-01"]));
    assert_ne!(with_lock, untracked);
    let provenance: serde_json::Value =
        serde_json::from_str(with_lock.lines().find_map(|l| l.strip_prefix("provenance: ")).unwrap()).unwrap();
    assert!(
        provenance["repositories"]["root"]["build_context"]["Cargo.lock"]
            .as_str()
            .is_some_and(|h| h.starts_with("blake3:"))
    );
}

#[test]
fn pack_records_every_repository_and_is_independent_of_repo_argument_order() {
    if !has_git() {
        return;
    }
    let left = scratch("pack-repo-left");
    let right = scratch("pack-repo-right");
    for dir in [&left, &right] {
        git(dir, &["init", "-q"]);
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "-m", "base"]);
    }
    let a = format!("left={}", left.display());
    let b = format!("right={}", right.display());
    let first = sealmap(&left, &["pack", "LED-01", "--repo", &a, "--repo", &b]);
    assert_eq!(code(&first), 0, "{}", String::from_utf8_lossy(&first.stderr));
    let reversed = sealmap(&left, &["pack", "LED-01", "--repo", &b, "--repo", &a]);
    assert_eq!(first.stdout, reversed.stdout);
    let text = stdout(&first);
    let provenance: serde_json::Value =
        serde_json::from_str(text.lines().find_map(|l| l.strip_prefix("provenance: ")).unwrap()).unwrap();
    assert!(provenance["repositories"]["left"]["head"].as_str().is_some_and(|s| s.len() == 40));
    assert!(provenance["repositories"]["right"]["head"].as_str().is_some_and(|s| s.len() == 40));
    fs::write(right.join("src/untracked.rs"), "pub fn additional() {}\n").unwrap();
    let changed = stdout(&sealmap(&left, &["pack", "LED-01", "--repo", &a, "--repo", &b]));
    assert_ne!(text, changed);
    let provenance: serde_json::Value =
        serde_json::from_str(changed.lines().find_map(|l| l.strip_prefix("provenance: ")).unwrap()).unwrap();
    assert_eq!(provenance["repositories"]["right"]["untracked"], serde_json::json!(["src/untracked.rs"]));
    assert_eq!(provenance["repositories"]["left"]["untracked"], serde_json::json!([]));
}

#[test]
fn pack_diff_supports_declared_non_rust_sources_and_topic_edits() {
    if !has_git() {
        return;
    }
    let dir = scratch("pack-declared-sources");
    fs::create_dir_all(dir.join("web")).unwrap();
    fs::write(dir.join("web/client.ts"), "export const active = true;\n").unwrap();
    fs::write(
        dir.join("docs/diagrams/ledger/01-accounts.md"),
        "---\nid: LED-01\nsources:\n  - web/client.ts\n---\nClient active.\n",
    )
    .unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    assert!(stdout(&sealmap(&dir, &["pack", "--diff", "HEAD"])).is_empty());
    fs::write(dir.join("web/client.ts"), "export const active = false;\n").unwrap();
    assert!(stdout(&sealmap(&dir, &["pack", "--diff", "HEAD"])).contains("topics: LED-01"));
    git(&dir, &["commit", "-q", "-am", "client"]);
    edit(&dir, "docs/diagrams/ledger/01-accounts.md", "Client active.", "Client inactive.");
    assert!(stdout(&sealmap(&dir, &["pack", "--diff", "HEAD"])).contains("Client inactive."));
}

#[test]
fn empty_diff_and_existing_outputs_never_report_a_fresh_pack() {
    if !has_git() {
        return;
    }
    let dir = scratch("pack-output-freshness");
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    fs::write(dir.join("old-pack.txt"), "old snapshot").unwrap();
    let out = sealmap(&dir, &["pack", "--diff", "HEAD", "-o", "old-pack.txt"]);
    assert_eq!(code(&out), 1);
    assert_eq!(fs::read_to_string(dir.join("old-pack.txt")).unwrap(), "old snapshot");
    fs::create_dir(dir.join("packs")).unwrap();
    fs::write(dir.join("packs/pack-99.txt"), "old shard").unwrap();
    let out = sealmap(&dir, &["pack", "LED-01", "--shard", "--budget", "10000", "-o", "packs"]);
    assert_ne!(code(&out), 0);
    assert_eq!(fs::read_dir(dir.join("packs")).unwrap().count(), 1);
    assert_eq!(fs::read_to_string(dir.join("packs/pack-99.txt")).unwrap(), "old shard");
    let out = sealmap(&dir, &["pack", "LED-01", "-o", "src/accounts.rs"]);
    assert_ne!(code(&out), 0);
    assert_eq!(fs::read_to_string(dir.join("src/accounts.rs")).unwrap(), ACCOUNTS);
}

#[cfg(unix)]
#[test]
fn pack_outputs_do_not_follow_existing_symlinks() {
    let dir = scratch("pack-output-symlinks");
    std::os::unix::fs::symlink("src/accounts.rs", dir.join("out.txt")).unwrap();
    assert_ne!(code(&sealmap(&dir, &["pack", "LED-01", "-o", "out.txt"])), 0);
    assert_eq!(fs::read_to_string(dir.join("src/accounts.rs")).unwrap(), ACCOUNTS);
    fs::create_dir(dir.join("foreign")).unwrap();
    std::os::unix::fs::symlink("foreign", dir.join("packs")).unwrap();
    assert_ne!(code(&sealmap(&dir, &["pack", "LED-01", "--shard", "--budget", "10000", "-o", "packs"])), 0);
    assert_eq!(fs::read_dir(dir.join("foreign")).unwrap().count(), 0);
}

/// ER `gen X-08`: every `--repo` is loaded under its `NAME/` prefix, so two
/// repositories with one name (or one name nested in another's prefix)
/// silently overwrote each other's files. Such a set is now refused.
#[test]
fn repos_with_clashing_names_are_refused() {
    let a = scratch("repo-clash-a");
    let b = scratch("repo-clash-b");
    edit(&b, "src/accounts.rs", "deposit", "credit");
    let (pa, pb) = (format!("api={}", a.display()), format!("api={}", b.display()));
    let o = sealmap(&a, &["model", "--repo", &pa, "--repo", &pb]);
    assert_eq!(code(&o), 2, "{}", stdout(&o));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("api"), "{err}");
    let nested = format!("api/v2={}", b.display());
    assert_ne!(code(&sealmap(&a, &["model", "--repo", &pa, "--repo", &nested])), 0);

    // The library refuses the same set.
    assert!(sealmap::load_repos(&[("api", a.as_path()), ("api", b.as_path())]).is_err());
    // Distinct names load both.
    let both = sealmap::load_repos(&[("api", a.as_path()), ("web", b.as_path())]).unwrap();
    assert!(both.get_str("api/src/accounts.rs").is_some() && both.get_str("web/src/accounts.rs").is_some());
}

#[test]
fn sign_refuses_a_same_family_review_and_writes_nothing() {
    let dir = scratch("sign-same-family");
    let o = sealmap(
        &dir,
        &["seal", "sign", "LED-01", "--reviewer", "claude:claude-fable-5-1", "--model", "claude:claude-sonnet-5-5"],
    );
    assert_eq!(code(&o), 1);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("same model family (anthropic)"), "{err}");
    assert!(!dir.join("docs/diagrams/seals.lock").exists());
}

#[test]
fn sign_refuses_a_multi_rung_reviewer_and_writes_nothing() {
    let dir = scratch("sign-multi-reviewer");
    let o = sealmap(
        &dir,
        &[
            "seal",
            "sign",
            "LED-01",
            "--reviewer",
            "zai:glm-5.3 -> claude:claude-fable-5-1",
            "--model",
            "claude:claude-sonnet-5-5",
        ],
    );
    assert_eq!(code(&o), 1);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("exactly one"), "{err}");
    assert!(!dir.join("docs/diagrams/seals.lock").exists());
}

#[cfg(unix)]
#[test]
fn lock_links_and_diagram_parent_links_refuse_without_disclosure_or_partial_signing() {
    use std::os::unix::fs::symlink;
    let args = ["seal", "sign", "LED-01", "--reviewer", "zai:glm-5.3", "--model", "claude:claude-sonnet-5-5"];
    for dangling in [false, true] {
        let dir = scratch(if dangling { "lock-link-dangling" } else { "lock-link-secret" });
        let target = dir.join("fixture-secret.txt");
        if !dangling {
            fs::write(&target, "FIXTURE_SECRET_DO_NOT_LOG = [malformed").unwrap();
        }
        symlink(&target, dir.join("docs/diagrams/seals.lock")).unwrap();
        for command in [&["verify"][..], &args[..]] {
            let output = sealmap(&dir, command);
            assert_eq!(code(&output), 2, "{}", String::from_utf8_lossy(&output.stderr));
            assert!(!stdout(&output).contains("FIXTURE_SECRET_DO_NOT_LOG"));
            assert!(!String::from_utf8_lossy(&output.stderr).contains("FIXTURE_SECRET_DO_NOT_LOG"));
        }
        assert_eq!(fs::read_to_string(dir.join("docs/diagrams/ledger/01-accounts.md")).unwrap(), TOPIC);
        assert_eq!(target.exists(), !dangling);
        if !dangling {
            assert_eq!(fs::read_to_string(target).unwrap(), "FIXTURE_SECRET_DO_NOT_LOG = [malformed");
        }
    }
    let dir = scratch("diagrams-parent-link");
    fs::rename(dir.join("docs"), dir.join("outside-docs")).unwrap();
    symlink("outside-docs", dir.join("docs")).unwrap();
    assert_eq!(code(&sealmap(&dir, &args)), 2);
    assert_eq!(fs::read_to_string(dir.join("outside-docs/diagrams/ledger/01-accounts.md")).unwrap(), TOPIC);
    assert!(!dir.join("outside-docs/diagrams/seals.lock").exists());
}

#[test]
fn malformed_regular_lock_diagnostics_do_not_include_input_contents() {
    let dir = scratch("lock-malformed-redaction");
    fs::write(dir.join("docs/diagrams/seals.lock"), "FIXTURE_SECRET_DO_NOT_LOG = [malformed").unwrap();
    for args in [
        &["verify"][..],
        &["verify", "--json"][..],
        &["seal-check", "LED-01"][..],
        &["resolve", DEPOSIT][..],
        &["stale"][..],
    ] {
        let output = sealmap(&dir, args);
        assert!(!output.status.success());
        assert!(!stdout(&output).contains("FIXTURE_SECRET_DO_NOT_LOG"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("FIXTURE_SECRET_DO_NOT_LOG"));
    }
}

#[cfg(unix)]
#[test]
fn dense_refuses_final_and_parent_links_before_writing_any_output() {
    use std::os::unix::fs::symlink;
    for name in ["dense.txt", "_index.txt"] {
        let dir = scratch(&format!("dense-link-{name}"));
        fs::create_dir_all(dir.join(".sealmap/dense")).unwrap();
        fs::write(dir.join("foreign.txt"), "KEEP_FOREIGN_BYTES").unwrap();
        symlink(dir.join("foreign.txt"), dir.join(".sealmap/dense").join(name)).unwrap();
        assert_eq!(code(&sealmap(&dir, &["dense"])), 2);
        assert_eq!(fs::read_to_string(dir.join("foreign.txt")).unwrap(), "KEEP_FOREIGN_BYTES");
        assert_eq!(fs::read_dir(dir.join(".sealmap/dense")).unwrap().count(), 1);
    }
    let dir = scratch("dense-parent-link");
    fs::create_dir(dir.join("foreign")).unwrap();
    symlink("foreign", dir.join(".sealmap")).unwrap();
    assert_eq!(code(&sealmap(&dir, &["dense"])), 2);
    assert!(fs::read_dir(dir.join("foreign")).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn explicit_source_root_alias_and_regular_dense_outputs_remain_usable() {
    use std::os::unix::fs::symlink;
    let dir = scratch("source-root-alias");
    let alias = dir.with_extension("alias");
    let _ = fs::remove_file(&alias);
    symlink(&dir, &alias).unwrap();
    let output = sealmap(
        &dir,
        &[
            "seal",
            "sign",
            "LED-01",
            "-C",
            alias.to_str().unwrap(),
            "--reviewer",
            "zai:glm-5.3",
            "--model",
            "claude:claude-sonnet-5-5",
        ],
    );
    assert_eq!(code(&output), 0, "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(code(&sealmap(&dir, &["verify", "-C", alias.to_str().unwrap()])), 0);
    for _ in 0..2 {
        assert_eq!(code(&sealmap(&dir, &["dense", "-o", "new/nested/dense"])), 0);
    }
    assert!(!fs::read_to_string(dir.join("new/nested/dense/dense.txt")).unwrap().is_empty());
    fs::remove_file(alias).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn git_export_directory_is_private_and_removed_after_success_and_failure() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("git-private-export");
    sign(&dir);
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "fixture"]);
    let real_tar = Command::new("sh").args(["-c", "command -v tar"]).output().unwrap();
    let bin = dir.join("bin");
    fs::create_dir(&bin).unwrap();
    let temp = dir.join("temporary");
    fs::create_dir(&temp).unwrap();
    fs::write(bin.join("tar"), "#!/bin/sh\n[ \"$(stat -c %a \"$3\")\" = 700 ] || exit 47\n[ -z \"$SEALMAP_FAIL_TAR\" ] || exit 48\nexec \"$SEALMAP_REAL_TAR\" \"$@\"\n").unwrap();
    fs::set_permissions(bin.join("tar"), fs::Permissions::from_mode(0o700)).unwrap();
    for fail in [false, true] {
        let output = Command::new("sh")
            .current_dir(&dir)
            .args(["-c", "umask 022; exec \"$@\"", "export-test"])
            .arg(env!("CARGO_BIN_EXE_sealmap"))
            .args(["stale", "--since", "HEAD"])
            .env("GIT_CEILING_DIRECTORIES", GIT_CEILING)
            .env("TMPDIR", &temp)
            .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
            .env("SEALMAP_REAL_TAR", String::from_utf8_lossy(&real_tar.stdout).trim())
            .env("SEALMAP_FAIL_TAR", if fail { "yes" } else { "" })
            .output()
            .unwrap();
        assert_eq!(code(&output), if fail { 2 } else { 0 }, "{}", String::from_utf8_lossy(&output.stderr));
        assert!(fs::read_dir(&temp).unwrap().next().is_none(), "temporary export must be removed");
    }
}

#[cfg(unix)]
#[test]
fn all_cli_output_modes_refuse_symlinked_parent_directories() {
    let dir = scratch("output-parent-links");
    fs::create_dir(dir.join("foreign")).unwrap();
    std::os::unix::fs::symlink("foreign", dir.join("linked")).unwrap();
    for args in [
        vec!["generate", "-o", "linked/generated"],
        vec!["pack", "LED-01", "-o", "linked/pack.txt"],
        vec!["pack", "LED-01", "--shard", "-o", "linked/packs"],
    ] {
        assert_eq!(code(&sealmap(&dir, &args)), 2);
        assert_eq!(fs::read_dir(dir.join("foreign")).unwrap().count(), 0);
    }
    assert_eq!(code(&sealmap(&dir, &["generate", "-o", "regular/nested"])), 0);
    assert_eq!(code(&sealmap(&dir, &["generate", "--check", "-o", "regular/nested"])), 0);
    assert_eq!(code(&sealmap(&dir, &["pack", "LED-01", "-o", "regular/pack.txt"])), 0);
}
