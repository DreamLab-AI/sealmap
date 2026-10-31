//! The seal surface end to end over a realistic fixture crate: sign, then
//! every row of the verify table, `stale` across two trees, `resolve`, and
//! the canonical lock writer.

use std::collections::{BTreeMap, BTreeSet};

use sealmap_corpus::seal::{
    self, Class, Lock, LockError, Resolution, SealReport, Signature, SymbolSeal, Topics, resolve, seal_check, stale,
    verify,
};
use sealmap_model::{Codebase, Fingerprint, SourcePath, SourceSet, SymbolId};
use sealmap_rust::{RustOptions, extract};

const LIB: &str = "pub mod accounts;\npub mod journal;\n";

const ACCOUNTS: &str = r#"
/// A ledger account.
pub struct LedgerAccount {
    pub balance: i64,
    pub frozen: bool,
}

impl LedgerAccount {
    pub fn deposit(&mut self, amount: i64) {
        self.balance += amount;
    }

    pub fn withdraw(&mut self, amount: i64) -> Result<(), String> {
        if self.frozen || amount > self.balance {
            return Err("refused".into());
        }
        self.balance -= amount;
        Ok(())
    }
}
"#;

const JOURNAL: &str = r#"
use crate::accounts::LedgerAccount;

pub struct JournalEntry {
    pub amount: i64,
}

impl JournalEntry {
    pub fn post_to(&self, account: &mut LedgerAccount) {
        account.deposit(self.amount);
        audit_trail(self.amount);
    }
}

fn audit_trail(amount: i64) -> i64 {
    amount * 2 + 1
}
"#;

const DEPOSIT: &str = "sym:cargo ledger . accounts/LedgerAccount#deposit().";
const WITHDRAW: &str = "sym:cargo ledger . accounts/LedgerAccount#withdraw().";
const POST_TO: &str = "sym:cargo ledger . journal/JournalEntry#post_to().";
const ACCOUNTS_MOD: &str = "sym:cargo ledger . accounts/";

const TOPIC: &str = "---
id: LED-01
title: Posting a journal entry
area: ledger
---
## For developers

A journal entry is posted with `sym:cargo ledger . journal/JournalEntry#post_to().`,
which calls `sym:cargo ledger . accounts/LedgerAccount#deposit().`; withdrawals go
through `sym:cargo ledger . accounts/LedgerAccount#withdraw().` and are refused on a
frozen account. The module is `sym:cargo ledger . accounts/`.

Grammar mentions such as `sym:? deposit` and `sym:extern std::mem` are not citations.

```mermaid
sequenceDiagram
    JournalEntry->>LedgerAccount: deposit (`sym:cargo ledger . not/Cited#`)
```
";

/// A second topic with no citations: sealed for its prose alone.
const NOTES: &str = "---\nid: LED-02\ntitle: Notes\narea: ledger\n---\nNo symbols here.\n";

fn tree(files: &[(&str, &str)]) -> Codebase {
    let mut src = SourceSet::new();
    src.insert("Cargo.toml", "[package]\nname = \"ledger\"\n").unwrap();
    for (p, t) in files {
        src.insert(p, t).unwrap();
    }
    extract(&src, &RustOptions::default()).codebase
}

fn base() -> Codebase {
    tree(&[("src/lib.rs", LIB), ("src/accounts.rs", ACCOUNTS), ("src/journal.rs", JOURNAL)])
}

fn with(file: &str, text: &str) -> Codebase {
    let mut files = vec![("src/lib.rs", LIB), ("src/accounts.rs", ACCOUNTS), ("src/journal.rs", JOURNAL)];
    files.iter_mut().find(|(p, _)| *p == file).expect("fixture file").1 = text;
    tree(&files)
}

fn path(p: &str) -> SourcePath {
    SourcePath::new(p).unwrap()
}

fn id(s: &str) -> SymbolId {
    SymbolId::parse(s).unwrap()
}

fn who() -> Signature {
    Signature { reviewer: "zai:glm-5.3".into(), model: "claude:claude-sonnet-5-5".into(), date: "2026-10-05".into() }
}

/// Sign both topics against `codebase`; returns the lock text and topics.
fn sealed(codebase: &Codebase) -> (String, Topics) {
    let mut lock = Lock::new();
    let mut topics = Topics::new();
    for (file, text) in [("ledger/01-posting.md", TOPIC), ("ledger/02-notes.md", NOTES)] {
        let t = seal::sign(&mut lock, &path(file), text, codebase, seal::LOCK_FILE, &who()).unwrap();
        topics.insert(path(file), t);
    }
    (lock.to_toml(), topics)
}

fn classes(report: &SealReport) -> BTreeMap<(String, String), Class> {
    report
        .findings
        .iter()
        .map(|f| ((f.topic.clone().unwrap_or_default(), f.symbol.clone().unwrap_or_default()), f.class))
        .collect()
}

fn class_of(report: &SealReport, symbol: &str) -> Vec<Class> {
    report.findings.iter().filter(|f| f.symbol.as_deref() == Some(symbol)).map(|f| f.class).collect()
}

fn failing(report: &SealReport) -> BTreeSet<Class> {
    report.failures().map(|f| f.class).collect()
}

// ------------------------------------------------------------------ holds

#[test]
fn a_fresh_seal_holds_and_records_every_global_citation() {
    let cb = base();
    let (lock_text, topics) = sealed(&cb);
    let lock = Lock::parse_canonical(&lock_text).unwrap();
    let entry = &lock.topics["LED-01"];
    let ids: Vec<_> = entry.symbols.keys().map(SymbolId::to_string).collect();
    // Sorted, deduplicated, global citations only; the fenced example is not one.
    assert_eq!(ids, [ACCOUNTS_MOD, DEPOSIT, WITHDRAW, POST_TO]);
    assert_eq!(entry.reviewer, "zai:glm-5.3");
    assert!(lock.topics["LED-02"].symbols.is_empty());
    assert!(topics[&path("ledger/01-posting.md")].contains("area: ledger\nsealed: seals.lock\n---\n"));

    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    assert!(report.passes(), "{report:#?}");
    // Four symbols + two prose findings, all holding.
    assert_eq!(report.count(Class::Holds), 6);
}

#[test]
fn moving_and_reformatting_code_still_holds() {
    let cb = base();
    let (lock_text, topics) = sealed(&cb);
    // Swap the two methods, reflow and comment them: ids and hashes are blind to it.
    let moved = r#"
// moved around
pub struct LedgerAccount { pub balance: i64, pub frozen: bool }
impl LedgerAccount {
    /// Takes money out.
    pub fn withdraw(&mut self, amount: i64) -> Result<(), String> {
        if self.frozen || amount > self.balance { return Err("refused".into()); }
        self.balance -= amount; Ok(())
    }
}
impl LedgerAccount { pub fn deposit(&mut self, amount: i64) { self.balance += amount; } }
"#;
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &with("src/accounts.rs", moved));
    for s in [DEPOSIT, WITHDRAW, POST_TO] {
        assert_eq!(class_of(&report, s), [Class::Holds], "{s}");
    }
    // Known limit, pinned: a module's body folds its members in source order,
    // so reordering items inside it changes the module's body hash.
    assert_eq!(class_of(&report, ACCOUNTS_MOD), [Class::Behaviour]);
    assert_eq!(failing(&report), BTreeSet::from([Class::Behaviour]));
}

// ------------------------------------------------------------------ code classes

#[test]
fn a_body_edit_is_behaviour() {
    let (lock_text, topics) = sealed(&base());
    let cb = with("src/accounts.rs", &ACCOUNTS.replace("self.balance += amount;", "self.balance += amount * 2;"));
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    assert_eq!(class_of(&report, DEPOSIT), [Class::Behaviour]);
    assert_eq!(class_of(&report, WITHDRAW), [Class::Holds]);
    // The module's body folds its members, so it moves too (MOD O-05).
    assert_eq!(class_of(&report, ACCOUNTS_MOD), [Class::Behaviour]);
    assert!(!report.passes());
}

#[test]
fn a_signature_edit_is_contract() {
    let (lock_text, topics) = sealed(&base());
    let cb = with("src/accounts.rs", &ACCOUNTS.replace("-> Result<(), String>", "-> Result<(), &'static str>"));
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    assert_eq!(class_of(&report, WITHDRAW), [Class::Contract]);
    assert_eq!(class_of(&report, DEPOSIT), [Class::Holds]);
}

#[test]
fn a_rename_is_absent_with_the_renamed_symbol_as_candidate() {
    let (lock_text, topics) = sealed(&base());
    let cb = with("src/journal.rs", &JOURNAL.replace("fn post_to(", "fn apply_to("));
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    let f = report.findings.iter().find(|f| f.symbol.as_deref() == Some(POST_TO)).unwrap();
    assert_eq!(f.class, Class::Absent);
    assert_eq!(f.candidates, [id("sym:cargo ledger . journal/JournalEntry#apply_to().")]);
    assert!(f.detail.contains("rename suspected"));
}

#[test]
fn a_deletion_is_absent_with_no_candidates() {
    let (lock_text, topics) = sealed(&base());
    let gone = JOURNAL.replace(
        "pub fn post_to(&self, account: &mut LedgerAccount) {",
        "pub fn other(&self, account: &mut LedgerAccount) { let _ = 1;",
    );
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &with("src/journal.rs", &gone));
    let f = report.findings.iter().find(|f| f.symbol.as_deref() == Some(POST_TO)).unwrap();
    assert_eq!(f.class, Class::Absent);
    assert!(f.candidates.is_empty());
}

#[test]
fn rename_candidates_are_restricted_to_the_same_kind() {
    // A free function with the sealed method's body is not a candidate.
    let (lock_text, topics) = sealed(&base());
    let cb = with(
        "src/journal.rs",
        &JOURNAL.replace("fn post_to(", "fn renamed_away(").replace(
            "fn audit_trail(",
            "pub fn post_free(entry: &JournalEntry, account: &mut LedgerAccount) { account.deposit(entry.amount); audit_trail(entry.amount); }\nfn audit_trail(",
        ),
    );
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    let f = report.findings.iter().find(|f| f.symbol.as_deref() == Some(POST_TO)).unwrap();
    assert_eq!(f.candidates, [id("sym:cargo ledger . journal/JournalEntry#renamed_away().")]);
}

#[test]
fn an_unparsable_file_fails_closed() {
    let (lock_text, topics) = sealed(&base());
    let broken =
        ACCOUNTS.replace("pub fn deposit(&mut self, amount: i64) {", "pub fn deposit(&mut self, amount: i64 {");
    let cb = with("src/accounts.rs", &broken);
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    // The symbols it held are not called absent, and the module itself is not called behaviour.
    for s in [DEPOSIT, WITHDRAW, ACCOUNTS_MOD] {
        assert_eq!(class_of(&report, s), [Class::Unparsable], "{s}");
    }
    // Symbols in files that do parse are still judged normally.
    assert_eq!(class_of(&report, POST_TO), [Class::Holds]);
    assert!(!report.passes());

    // resolve agrees, and sign refuses to seal across the broken file.
    assert_eq!(resolve(&cb, &id(DEPOSIT), None), Resolution::Unparsable { file: path("src/accounts.rs") });
    let mut lock = Lock::new();
    let err = seal::sign(&mut lock, &path("ledger/01-posting.md"), TOPIC, &cb, seal::LOCK_FILE, &who()).unwrap_err();
    assert!(err.to_string().contains("does not parse"), "{err}");
    assert!(lock.topics.is_empty());
}

// ------------------------------------------------------------------ corpus classes

#[test]
fn a_new_citation_is_unsealed_and_edits_the_prose() {
    let cb = base();
    let (lock_text, mut topics) = sealed(&cb);
    let t = topics.get_mut(&path("ledger/01-posting.md")).unwrap();
    *t = t.replace("The module is", "Auditing is `sym:cargo ledger . journal/audit_trail().`. The module is");
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    assert_eq!(class_of(&report, "sym:cargo ledger . journal/audit_trail()."), [Class::UnsealedCitation]);
    assert_eq!(classes(&report)[&("LED-01".to_owned(), String::new())], Class::ProseEdited);
    assert_eq!(failing(&report), BTreeSet::from([Class::UnsealedCitation, Class::ProseEdited]));
}

#[test]
fn a_malformed_global_citation_is_unsealed_and_refused_by_sign() {
    let cb = base();
    // A bare name may not contain a space: this is a typo, not an id.
    let bad =
        TOPIC.replace("frozen account.", "frozen account (`sym:cargo ledger . accounts/Ledger Account#freeze().`).");
    let topics: Topics = [(path("ledger/01-posting.md"), bad.clone())].into_iter().collect();
    let report = verify(None, seal::LOCK_FILE, &topics, &cb);
    let f = report.failures().find(|f| f.symbol.as_deref().is_some_and(|s| s.contains("Ledger Account"))).unwrap();
    assert_eq!(f.class, Class::UnsealedCitation);
    assert!(f.detail.contains("not a canonical sym: id"), "{}", f.detail);
    let err =
        seal::sign(&mut Lock::new(), &path("ledger/01-posting.md"), &bad, &cb, seal::LOCK_FILE, &who()).unwrap_err();
    assert!(matches!(err, seal::SignError::Malformed { .. }), "{err}");
}

#[test]
fn an_unsealed_topic_with_citations_fails_and_one_without_is_coverage_only() {
    let cb = base();
    let topics: Topics =
        [(path("ledger/01-posting.md"), TOPIC.to_owned()), (path("ledger/02-notes.md"), NOTES.to_owned())]
            .into_iter()
            .collect();
    let report = verify(None, seal::LOCK_FILE, &topics, &cb);
    assert_eq!(report.count(Class::UnsealedCitation), 4);
    assert_eq!(report.unsealed_topics, [path("ledger/02-notes.md")]);
    // Without the citing topic, the gate passes: legacy topics migrate one at a time.
    let legacy: Topics = [(path("ledger/02-notes.md"), NOTES.to_owned())].into_iter().collect();
    assert!(verify(None, seal::LOCK_FILE, &legacy, &cb).passes());
}

#[test]
fn a_missing_topic_file_and_an_uncited_symbol_are_orphans() {
    let cb = base();
    let (lock_text, mut topics) = sealed(&cb);
    topics.remove(&path("ledger/02-notes.md"));
    // Drop a citation from the prose: its sealed symbol is now an orphan (and the prose changed).
    let t = topics.get_mut(&path("ledger/01-posting.md")).unwrap();
    *t = t.replace(" The module is `sym:cargo ledger . accounts/`.", "");
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    assert_eq!(classes(&report)[&("LED-02".to_owned(), String::new())], Class::Orphan);
    assert_eq!(class_of(&report, ACCOUNTS_MOD), [Class::Holds, Class::Orphan]);
    assert_eq!(failing(&report), BTreeSet::from([Class::Orphan, Class::ProseEdited]));
}

#[test]
fn prose_edits_fail_but_pointer_and_line_ending_changes_do_not() {
    let cb = base();
    let (lock_text, mut topics) = sealed(&cb);
    let file = path("ledger/01-posting.md");
    let original = topics[&file].clone();

    topics.insert(file.clone(), original.replace('\n', "\r\n"));
    assert!(verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb).passes());

    topics.insert(file.clone(), original.replace("are refused", "are always refused"));
    let report = verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb);
    assert_eq!(failing(&report), BTreeSet::from([Class::ProseEdited]));
    // A trailing space is an edit too: the hash is over the bytes.
    topics.insert(file, original.replacen("## For developers", "## For developers ", 1));
    assert!(!verify(Some(&lock_text), seal::LOCK_FILE, &topics, &cb).passes());
}

#[test]
fn pointer_and_lock_disagreements_are_lock_faults() {
    let cb = base();
    let (lock_text, topics) = sealed(&cb);
    let file = path("ledger/01-posting.md");
    let t = topics[&file].clone();
    let fault_details = |text: String| {
        let mut ts = topics.clone();
        ts.insert(file.clone(), text);
        let r = verify(Some(&lock_text), seal::LOCK_FILE, &ts, &cb);
        r.findings.iter().filter(|f| f.class == Class::LockFault).map(|f| f.detail.clone()).collect::<Vec<_>>()
    };
    // Pointer removed (the prose still holds: the hash skips the pointer).
    let d = fault_details(t.replace("sealed: seals.lock\n", ""));
    assert!(d[0].contains("has no `sealed: seals.lock` line"), "{d:?}");
    // Pointer names another lock.
    let d = fault_details(t.replace("sealed: seals.lock", "sealed: other.lock"));
    assert!(d[0].contains("names `other.lock`"), "{d:?}");
    // Two pointers.
    let d = fault_details(t.replace("sealed: seals.lock\n", "sealed: seals.lock\nsealed: seals.lock\n"));
    assert!(d[0].contains("2 `sealed:` lines"), "{d:?}");
    // The file's id no longer matches its entry.
    let d = fault_details(t.replace("id: LED-01", "id: LED-09"));
    assert!(d.iter().any(|x| x.contains("front matter says `id: LED-09`")), "{d:?}");

    // A pointer with no entry.
    let mut ts = topics.clone();
    ts.insert(path("ledger/03-new.md"), "---\nid: LED-03\nsealed: seals.lock\n---\nx\n".into());
    let r = verify(Some(&lock_text), seal::LOCK_FILE, &ts, &cb);
    assert_eq!(failing(&r), BTreeSet::from([Class::LockFault]));
}

#[test]
fn a_non_canonical_lock_is_a_fault_but_still_checked() {
    let cb = base();
    let (lock_text, topics) = sealed(&cb);
    for bad in [
        lock_text.replace("version = 1", "version=1"),
        lock_text.replace('\n', "\r\n"),
        format!("{lock_text}\n"),
        lock_text.replace("[[topic]]\nid = \"LED-01\"", "[[topic]]\n# reviewed\nid = \"LED-01\""),
    ] {
        assert_eq!(Lock::parse_canonical(&bad), Err(LockError::NonCanonical));
        let r = verify(Some(&bad), seal::LOCK_FILE, &topics, &cb);
        assert_eq!(failing(&r), BTreeSet::from([Class::LockFault]), "{bad}");
        // Every entry was still classified.
        assert_eq!(r.count(Class::Holds), 6);
    }
}

#[test]
fn an_unreadable_lock_is_one_lock_fault() {
    let cb = base();
    let (lock_text, topics) = sealed(&cb);
    for (bad, want) in [
        (lock_text.replace("version = 1", "version = 2"), "version 2"),
        (lock_text.replace("\"sm1\"", "\"sm2\""), "algorithm `sm2`"),
        (lock_text.replace("reviewer =", "approver = \"x\"\nreviewer ="), "unknown field"),
        (lock_text.replace("date = \"2026-10-05\"", "date = \"5 Oct\""), "YYYY-MM-DD"),
        (lock_text.replacen("blake3-16:", "blake3-15:", 1), "topic_hash"),
        (lock_text.replace("file = \"ledger/02-notes.md\"", "file = \"ledger/01-posting.md\""), "sealed by two topics"),
        (lock_text.replace("id = \"LED-02\"", "id = \"LED-01\""), "sealed twice"),
        (lock_text.replace("file = \"ledger/02-notes.md\"", "file = \"ledger/../ledger/02-notes.md\""), "normalised"),
        ("not toml [".to_owned(), "not a valid seals.lock"),
    ] {
        let r = verify(Some(&bad), seal::LOCK_FILE, &topics, &cb);
        assert_eq!(r.findings.len(), 1, "{bad}");
        assert_eq!(r.findings[0].class, Class::LockFault);
        assert!(r.findings[0].detail.contains(want), "{} !~ {want}", r.findings[0].detail);
    }
}

#[test]
fn seal_check_scopes_to_the_named_entries() {
    let cb = base();
    let (lock_text, topics) = sealed(&cb);
    let lock = Lock::parse(&lock_text).unwrap();
    let only: BTreeSet<String> = ["LED-02".to_owned()].into();
    let broken = with("src/accounts.rs", &ACCOUNTS.replace("+= amount", "-= amount"));
    let r = seal_check(&lock, seal::LOCK_FILE, &topics, &broken, Some(&only));
    assert!(r.passes(), "LED-02 seals no symbols: {r:#?}");
    assert!(r.findings.iter().all(|f| f.topic.as_deref() == Some("LED-02")));
    let r = seal_check(&lock, seal::LOCK_FILE, &topics, &broken, None);
    assert!(!r.passes());
    let unknown: BTreeSet<String> = ["LED-99".to_owned()].into();
    let r = seal_check(&lock, seal::LOCK_FILE, &topics, &broken, Some(&unknown));
    assert_eq!(failing(&r), BTreeSet::from([Class::LockFault]));
}

// ------------------------------------------------------------------ stale

#[test]
fn stale_against_the_lock_lists_only_changed_symbols() {
    let (lock_text, _) = sealed(&base());
    let lock = Lock::parse(&lock_text).unwrap();
    assert!(stale(&lock, None, &base()).is_empty());
    let after = with("src/journal.rs", &JOURNAL.replace("fn post_to(", "fn apply_to("));
    let s = stale(&lock, None, &after);
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].topic.as_str(), s[0].class), ("LED-01", Class::Absent));
    assert_eq!(s[0].candidates, [id("sym:cargo ledger . journal/JournalEntry#apply_to().")]);
}

#[test]
fn stale_across_two_trees_ignores_what_changed_before_the_first() {
    // Sealed on `base`; tree A already changed deposit's body; tree B then changes withdraw's signature.
    let (lock_text, _) = sealed(&base());
    let lock = Lock::parse(&lock_text).unwrap();
    let accounts_a = ACCOUNTS.replace("self.balance += amount;", "self.balance += amount + 0;");
    let accounts_b = accounts_a
        .replace("amount: i64) -> Result", "amount: u64) -> Result")
        .replace("amount > self.balance", "amount as i64 > self.balance")
        .replace("self.balance -= amount;", "self.balance -= amount as i64;");
    let a = with("src/accounts.rs", &accounts_a);
    let b = with("src/accounts.rs", &accounts_b);

    let since_a: Vec<_> = stale(&lock, Some(&a), &b).into_iter().map(|s| (s.symbol.to_string(), s.class)).collect();
    // The module's own contract is its name and attributes; a member's signature reaches only its body.
    assert_eq!(since_a, [(ACCOUNTS_MOD.to_owned(), Class::Behaviour), (WITHDRAW.to_owned(), Class::Contract)]);

    let since_seal: Vec<_> = stale(&lock, None, &b).into_iter().map(|s| (s.symbol.to_string(), s.class)).collect();
    assert!(since_seal.contains(&(DEPOSIT.to_owned(), Class::Behaviour)));

    // Identical trees: nothing changed, whatever the lock says.
    assert!(stale(&lock, Some(&b), &b).is_empty());
}

#[test]
fn stale_falls_back_to_the_lock_for_symbols_the_old_tree_lacks_or_cannot_parse() {
    let (lock_text, _) = sealed(&base());
    let lock = Lock::parse(&lock_text).unwrap();
    let broken_before = with("src/accounts.rs", "pub struct LedgerAccount {");
    let changed = with("src/accounts.rs", &ACCOUNTS.replace("self.balance += amount;", "self.balance = 0;"));
    let s: Vec<_> =
        stale(&lock, Some(&broken_before), &changed).into_iter().map(|s| (s.symbol.to_string(), s.class)).collect();
    assert!(s.contains(&(DEPOSIT.to_owned(), Class::Behaviour)), "{s:?}");
}

// ------------------------------------------------------------------ resolve

#[test]
fn resolve_reports_span_and_hashes() {
    let cb = base();
    match resolve(&cb, &id(WITHDRAW), None) {
        Resolution::Found { file, span, sig, body } => {
            assert_eq!(file, path("src/accounts.rs"));
            assert_eq!((span.start_line, span.end_line), (13, 19));
            let sym = cb.symbol(&id(WITHDRAW)).unwrap();
            assert_eq!((sig, body), (sym.sig_hash, sym.body_hash));
        }
        other => panic!("{other:?}"),
    }
    let body = cb.symbol(&id(POST_TO)).unwrap().body_hash;
    let renamed = with("src/journal.rs", &JOURNAL.replace("fn post_to(", "fn apply_to("));
    assert_eq!(
        resolve(&renamed, &id(POST_TO), Some(body)),
        Resolution::Absent { candidates: vec![id("sym:cargo ledger . journal/JournalEntry#apply_to().")] }
    );
    assert_eq!(resolve(&renamed, &id(POST_TO), None), Resolution::Absent { candidates: vec![] });
    // An unset body never matches anything.
    assert_eq!(
        resolve(&renamed, &id(POST_TO), Some(Fingerprint::default())),
        Resolution::Absent { candidates: vec![] }
    );
}

// ------------------------------------------------------------------ sign and the writer

#[test]
fn sign_refuses_what_verify_could_not_confirm() {
    let cb = base();
    let file = path("ledger/01-posting.md");
    let missing =
        TOPIC.replace("frozen account.", "frozen account (`sym:cargo ledger . accounts/LedgerAccount#freeze().`).");
    let err = seal::sign(&mut Lock::new(), &file, &missing, &cb, seal::LOCK_FILE, &who()).unwrap_err();
    assert!(err.to_string().contains("not in the model"), "{err}");
    let bad_date = Signature { date: "2026-13-01".into(), ..who() };
    assert!(seal::sign(&mut Lock::new(), &file, TOPIC, &cb, seal::LOCK_FILE, &bad_date).is_err());
    let no_reviewer = Signature { reviewer: " ".into(), ..who() };
    assert!(seal::sign(&mut Lock::new(), &file, TOPIC, &cb, seal::LOCK_FILE, &no_reviewer).is_err());
    assert!(seal::sign(&mut Lock::new(), &file, "no front matter", &cb, seal::LOCK_FILE, &who()).is_err());
    // An id already sealed for another file.
    let mut lock = Lock::new();
    seal::sign(&mut lock, &file, TOPIC, &cb, seal::LOCK_FILE, &who()).unwrap();
    let err = seal::sign(&mut lock, &path("ledger/09-copy.md"), TOPIC, &cb, seal::LOCK_FILE, &who()).unwrap_err();
    assert!(matches!(err, seal::SignError::IdTaken { .. }), "{err}");
}

#[test]
fn sign_refuses_a_same_family_or_unknown_family_review() {
    let cb = base();
    let file = path("ledger/01-posting.md");
    let same =
        Signature { reviewer: "claude:claude-fable-5-1".into(), model: "claude:claude-sonnet-5-5".into(), ..who() };
    let mut lock = Lock::new();
    let err = seal::sign(&mut lock, &file, TOPIC, &cb, seal::LOCK_FILE, &same).unwrap_err();
    assert!(matches!(err, seal::SignError::Family(_)), "{err}");
    assert!(err.to_string().contains("same model family"), "{err}");
    assert!(lock.topics.is_empty(), "a refused seal writes nothing");
    let unknown = Signature { reviewer: "r".into(), model: "m".into(), ..who() };
    assert!(matches!(
        seal::sign(&mut lock, &file, TOPIC, &cb, seal::LOCK_FILE, &unknown).unwrap_err(),
        seal::SignError::Family(_)
    ));
    assert!(lock.topics.is_empty());
}

#[test]
fn re_signing_replaces_the_entry_and_follows_a_renumbered_topic() {
    let cb = base();
    let file = path("ledger/01-posting.md");
    let mut lock = Lock::new();
    seal::sign(&mut lock, &file, TOPIC, &cb, seal::LOCK_FILE, &who()).unwrap();
    let renumbered = TOPIC.replace("id: LED-01", "id: LED-05");
    seal::sign(&mut lock, &file, &renumbered, &cb, seal::LOCK_FILE, &who()).unwrap();
    assert_eq!(lock.topics.keys().collect::<Vec<_>>(), ["LED-05"]);
}

#[test]
fn the_lock_writer_is_deterministic_and_round_trips() {
    let cb = base();
    let files = [("ledger/02-notes.md", NOTES), ("ledger/01-posting.md", TOPIC)];
    let sign_in = |order: &[(&str, &str)]| {
        let mut lock = Lock::new();
        for (f, t) in order {
            seal::sign(&mut lock, &path(f), t, &cb, seal::LOCK_FILE, &who()).unwrap();
        }
        lock.to_toml()
    };
    let forward = sign_in(&files);
    let reversed = sign_in(&[files[1], files[0]]);
    assert_eq!(forward, reversed);
    // Re-serialising a parsed lock is byte-identical, twice over.
    let again = Lock::parse_canonical(&forward).unwrap().to_toml();
    assert_eq!(again, forward);
    assert_eq!(Lock::parse_canonical(&again).unwrap().to_toml(), forward);

    // The exact layout, pinned.
    let sym = |s: &str| cb.symbol(&id(s)).unwrap().clone();
    let w = sym(WITHDRAW);
    let mut lock = Lock::new();
    lock.generator = "sealmap test".into();
    lock.insert(seal::TopicSeal {
        id: "LED-01".into(),
        file: path("ledger/01-posting.md"),
        topic_hash: Fingerprint::from_bytes([0xab; 16]),
        reviewer: "r \"q\"".into(),
        model: "m\\n".into(),
        date: "2026-10-05".into(),
        symbols: [(
            w.id.clone(),
            SymbolSeal { sig: Fingerprint::from_bytes([1; 16]), body: Fingerprint::from_bytes([2; 16]) },
        )]
        .into(),
    });
    let ab = "ab".repeat(16);
    let (one, two) = ("01".repeat(16), "02".repeat(16));
    assert_eq!(
        lock.to_toml(),
        format!(
            "version = 1\nalgorithm = \"sm1\"\ngenerator = \"sealmap test\"\n\n[[topic]]\nid = \"LED-01\"\nfile = \"ledger/01-posting.md\"\ntopic_hash = \"blake3-16:{ab}\"\nreviewer = \"r \\\"q\\\"\"\nmodel = \"m\\\\n\"\ndate = \"2026-10-05\"\nsymbols = [\n  {{ id = \"{WITHDRAW}\", sig = \"blake3-16:{one}\", body = \"blake3-16:{two}\" }},\n]\n"
        )
    );
}

#[test]
fn the_empty_lock_is_canonical() {
    let text = Lock::new().to_toml();
    assert_eq!(text, format!("version = 1\nalgorithm = \"sm1\"\ngenerator = \"{}\"\n", seal::GENERATOR));
    assert_eq!(Lock::parse_canonical(&text).unwrap(), Lock::new());
}

#[test]
fn sign_refuses_a_multi_rung_reviewer_and_writes_nothing() {
    let cb = base();
    let file = path("ledger/01-posting.md");
    let multi = Signature {
        reviewer: "zai:glm-5.3 -> claude:claude-fable-5-1".into(),
        model: "claude:claude-sonnet-5-5".into(),
        ..who()
    };
    let mut lock = Lock::new();
    let err = seal::sign(&mut lock, &file, TOPIC, &cb, seal::LOCK_FILE, &multi).unwrap_err();
    assert!(matches!(err, seal::SignError::BadSignature(_)), "{err}");
    assert!(err.to_string().contains("exactly one"), "{err}");
    assert!(lock.topics.is_empty(), "a refused seal writes nothing");
}

/// The three `diagram-*` author routes, verbatim from agentbox.toml
/// `[model_routing.routes]`. The single reviewer that gave the verdict
/// (`zai:glm-5.3`) must pass against every rung of each.
#[test]
fn the_single_glm_reviewer_passes_against_each_agentbox_diagram_author_route() {
    let cb = base();
    let file = path("ledger/01-posting.md");
    for author in [
        "loom:qwen3.8-27B -> claude:claude-sonnet-5-5",
        "claude:claude-sonnet-5-5 -> claude:claude-opus-5-5",
        "claude:claude-opus-5-5 -> claude:claude-fable-5-1",
    ] {
        let sig = Signature { reviewer: "zai:glm-5.3".into(), model: author.into(), ..who() };
        let mut lock = Lock::new();
        seal::sign(&mut lock, &file, TOPIC, &cb, seal::LOCK_FILE, &sig).unwrap_or_else(|e| panic!("{author}: {e}"));
        assert_eq!(lock.topics.len(), 1);
    }
}
