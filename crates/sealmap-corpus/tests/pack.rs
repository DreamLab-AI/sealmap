//! Review packs on a fixture corpus: the golden pack, determinism, budget
//! refusal, sharding and change selection.
//!
//! The golden file lives in `tests/golden/`. After an intended format change,
//! regenerate it with `SEALMAP_PACK_BLESS=1 cargo test -p sealmap-corpus
//! --test pack` and review the diff.

use std::path::PathBuf;

use sealmap_corpus::pack::{PackError, PackInput, PackOptions, TopicSize, changed_topics, pack, shard};
use sealmap_corpus::seal::{Lock, Signature, Topics, sign};
use sealmap_model::{Codebase, SourcePath, SourceSet};
use sealmap_rust::{RustOptions, extract};

const LIB: &str = "pub mod accounts;\npub mod audit;\n";

const ACCOUNTS: &str = "\
use crate::audit::Trail;

pub struct LedgerAccount {
    pub balance: i64,
    trail: Trail,
}

impl LedgerAccount {
    pub fn deposit(&mut self, amount: i64) {
        self.balance += amount;
        self.trail.record(\"deposit\", amount);
    }

    pub fn withdraw(&mut self, amount: i64) -> bool {
        if amount > self.balance {
            self.trail.record(\"refused\", amount);
            return false;
        }
        self.balance -= amount;
        self.trail.record(\"withdraw\", amount);
        true
    }
}
";

const AUDIT: &str = "\
pub struct Trail {
    entries: Vec<(String, i64)>,
}

impl Trail {
    pub fn record(&mut self, what: &str, amount: i64) {
        self.entries.push((what.to_owned(), amount));
    }
}
";

const LED01: &str = "---
id: LED-01
title: Accounts
area: ledger
---
Money goes in through `sym:cargo ledger . accounts/LedgerAccount#deposit().` and
out through `sym:cargo ledger . accounts/LedgerAccount#withdraw().`, which
refuses an overdraft. Deposits are recorded again by
`sym:cargo ledger . accounts/LedgerAccount#deposit().`.

Interest was paid by `sym:cargo ledger . accounts/LedgerAccount#credit().`.";

const LED02: &str = "---
id: LED-02
title: The audit trail
area: ledger
---
Every movement lands in `sym:cargo ledger . audit/Trail#record().`, owned by
`sym:cargo ledger . audit/Trail#`. A typo stays visible: `sym:cargo ledger . audit/Trail#record(`.
";

fn sources() -> SourceSet {
    let mut src = SourceSet::new();
    src.insert("Cargo.toml", "[package]\nname = \"ledger\"\n").unwrap();
    src.insert("src/lib.rs", LIB).unwrap();
    src.insert("src/accounts.rs", ACCOUNTS).unwrap();
    src.insert("src/audit.rs", AUDIT).unwrap();
    src
}

fn model(src: &SourceSet) -> Codebase {
    extract(src, &RustOptions { name: "ledger".into(), ..Default::default() }).codebase
}

fn topics() -> Topics {
    let mut t = Topics::new();
    t.insert(SourcePath::new("ledger/02-audit.md").unwrap(), LED02.into());
    t.insert(SourcePath::new("ledger/01-accounts.md").unwrap(), LED01.into());
    t
}

fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|s| (*s).to_owned()).collect()
}

/// Compare with `tests/golden/<name>`, or rewrite it under
/// `SEALMAP_PACK_BLESS=1`.
fn golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("SEALMAP_PACK_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with SEALMAP_PACK_BLESS=1 to create it)", path.display()));
    assert!(
        expected == actual,
        "{name} differs from the golden file; rerun with SEALMAP_PACK_BLESS=1 and review the diff\n--- actual ---\n{actual}"
    );
}

#[test]
fn golden_ledger_pack() {
    let src = sources();
    let cb = model(&src);
    let t = topics();
    let input = PackInput::new(&cb, &src, &t, "4f1a9de0c0ffee");
    let out = pack(&input, &ids(&["LED-02", "LED-01"]), &PackOptions::default().source_window(6)).unwrap();
    golden("ledger.pack.txt", &out.text);
    assert_eq!(out.header + out.topics.iter().map(|t| t.bytes).sum::<usize>(), out.text.len());
    assert_eq!(out.topics.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["LED-01", "LED-02"]);
}

#[test]
fn every_block_length_is_exact() {
    let src = sources();
    let cb = model(&src);
    let t = topics();
    let text =
        pack(&PackInput::new(&cb, &src, &t, "r"), &ids(&["LED-01", "LED-02"]), &PackOptions::default()).unwrap().text;
    // Walk the pack by the declared lengths alone: every block header is
    // followed by exactly its payload (plus a newline the length leaves out
    // when the payload lacks one), and the walk ends exactly at the end.
    let mut rest = text.split_once("source-window: 40 lines\n").unwrap().1;
    let mut blocks = 0;
    while !rest.is_empty() {
        let (head, after) = rest.split_once('\n').unwrap();
        assert!(head.starts_with("==== "), "not a block line: {head:?}");
        if head.starts_with("==== end ") {
            rest = after;
            continue;
        }
        let words: Vec<&str> = head.split(' ').collect();
        let at = words.iter().position(|w| *w == "bytes").unwrap();
        let n: usize = words[at - 1].parse().unwrap();
        let payload = &after[..n];
        rest = &after[n..];
        if !payload.is_empty() && !payload.ends_with('\n') {
            rest = rest.strip_prefix('\n').unwrap();
        }
        blocks += 1;
    }
    // Per topic: the topic, its slice, its unresolved list, two windows.
    assert_eq!(blocks, 10);
}

#[test]
fn identical_inputs_give_identical_bytes_whatever_the_order() {
    let src = sources();
    let cb = model(&src);
    let t = topics();
    let input = PackInput::new(&cb, &src, &t, "r");
    let opts = PackOptions::default();
    let a = pack(&input, &ids(&["LED-01", "LED-02"]), &opts).unwrap();
    let b = pack(&input, &ids(&["LED-02", "LED-01", "LED-02"]), &opts).unwrap();
    let c =
        pack(&PackInput::new(&model(&sources()), &src, &topics(), "r"), &ids(&["LED-02", "LED-01"]), &opts).unwrap();
    assert_eq!(a, b);
    assert_eq!(a, c);
    // The header records what shapes the bytes.
    let d = pack(&input.diff("main"), &ids(&["LED-01", "LED-02"]), &opts.clone().depth(2)).unwrap();
    assert!(d.text.contains("\nrevision: r\ndiff: main\ntopics: LED-01 LED-02\nbudget: none\ndepth: 2\n"));
}

#[test]
fn over_budget_is_refused_with_per_topic_sizes() {
    let src = sources();
    let cb = model(&src);
    let t = topics();
    let input = PackInput::new(&cb, &src, &t, "r");
    let full = pack(&input, &ids(&["LED-01", "LED-02"]), &PackOptions::default()).unwrap();
    let len = full.text.len();
    // The budget line is part of the header, so measure with a budget of
    // the same width.
    let at = |budget: usize| pack(&input, &ids(&["LED-01", "LED-02"]), &PackOptions::default().budget(budget));
    let exact = len + "budget: 9999 bytes\n".len() - "budget: none\n".len();
    let fits = at(exact).unwrap();
    assert_eq!(fits.text.len(), exact);
    match at(exact - 1).unwrap_err() {
        PackError::OverBudget { bytes, budget, header, topics } => {
            assert_eq!((bytes, budget), (exact, exact - 1));
            assert_eq!(header + topics.iter().map(|t| t.bytes).sum::<usize>(), bytes);
            assert_eq!(topics, fits.topics);
            let msg = at(exact - 1).unwrap_err().to_string();
            assert!(msg.contains("1 over the") && msg.contains("refusing rather than truncating"), "{msg}");
            assert!(msg.contains(&format!("LED-01 {}", topics[0].bytes)), "{msg}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn shards_hold_whole_topics_within_the_budget() {
    let src = sources();
    let cb = model(&src);
    let t = topics();
    let input = PackInput::new(&cb, &src, &t, "r");
    let all = ids(&["LED-01", "LED-02"]);
    let sizes = pack(&input, &all, &PackOptions::default()).unwrap().topics;
    let biggest = sizes.iter().map(|t| t.bytes).max().unwrap();
    let budget = biggest + 400;
    let packs = shard(&input, &all, &PackOptions::default().budget(budget)).unwrap();
    assert_eq!(packs.len(), 2);
    for (i, p) in packs.iter().enumerate() {
        assert!(p.text.len() <= budget);
        assert!(p.text.contains(&format!("\nshard: {}\n", i + 1)));
    }
    let shard_sizes: Vec<TopicSize> = packs.iter().flat_map(|p| p.topics.clone()).collect();
    assert_eq!(shard_sizes, sizes, "every topic in exactly one shard, unchanged");
    // A budget large enough for both gives one shard.
    assert_eq!(shard(&input, &all, &PackOptions::default().budget(1 << 20)).unwrap().len(), 1);
    // A topic that cannot fit alone is named.
    let err = shard(&input, &all, &PackOptions::default().budget(biggest)).unwrap_err();
    assert!(matches!(err, PackError::TopicOverBudget { .. }), "{err:?}");
}

#[test]
fn bad_selections_are_refused() {
    let src = sources();
    let cb = model(&src);
    let mut t = topics();
    let input = PackInput::new(&cb, &src, &t, "r");
    assert_eq!(pack(&input, &[], &PackOptions::default()).unwrap_err(), PackError::NoTopics);
    assert_eq!(
        pack(&input, &ids(&["LED-09", "LED-01", "AUD-01"]), &PackOptions::default()).unwrap_err(),
        PackError::UnknownTopics { ids: ids(&["AUD-01", "LED-09"]) }
    );
    t.insert(SourcePath::new("ledger/09-copy.md").unwrap(), LED01.into());
    let input = PackInput::new(&cb, &src, &t, "r");
    assert!(matches!(
        pack(&input, &ids(&["LED-01"]), &PackOptions::default()).unwrap_err(),
        PackError::DuplicateTopic { id, files } if id == "LED-01" && files.len() == 2
    ));
}

#[test]
fn changed_topics_follow_sealed_and_cited_symbols() {
    let before_src = sources();
    let before = model(&before_src);
    let t = topics();

    // A move (blank lines above everything) changes nothing.
    let mut moved = SourceSet::new();
    for (p, text) in before_src.iter() {
        moved.insert(p.as_str(), format!("\n\n{text}")).unwrap();
    }
    assert!(changed_topics(None, &t, &before, &model(&moved)).is_empty());

    // A body change in `record` touches LED-02 only.
    let mut edited = before_src.clone();
    edited.insert("src/audit.rs", AUDIT.replace("(what.to_owned(), amount)", "(what.into(), amount)")).unwrap();
    let after = model(&edited);
    assert_eq!(changed_topics(None, &t, &before, &after), ["LED-02"]);

    // A sealed topic whose sealed symbols changed counts even when the
    // change is to a symbol it no longer cites: the lock is consulted too.
    let mut lock = Lock::new();
    let who = Signature {
        reviewer: "zai:glm-5.3".into(),
        model: "claude:claude-sonnet-5-5".into(),
        date: "2026-10-06".into(),
    };
    let file = SourcePath::new("ledger/03-trail.md").unwrap();
    let sealed = "---\nid: LED-03\n---\nThe trail is `sym:cargo ledger . audit/Trail#record().`.\n";
    sign(&mut lock, &file, sealed, &before, "seals.lock", &who).unwrap();
    let mut t3 = t.clone();
    t3.insert(file, "---\nid: LED-03\nsealed: seals.lock\n---\nThe trail moved on.\n".into());
    assert_eq!(changed_topics(Some(&lock), &t3, &before, &after), ["LED-02", "LED-03"]);
    assert_eq!(changed_topics(None, &t3, &before, &after), ["LED-02"]);
}
