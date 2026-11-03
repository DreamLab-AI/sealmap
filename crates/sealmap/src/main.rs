//! `sealmap` command-line tool.
//!
//! ```text
//! sealmap generate [PATH] [-o .sealmap] [--check]   # write (or compare) the generated corpus
//! sealmap model    [PATH]                           # print the model as JSON
//! sealmap dense    [PATH] [-o .sealmap/dense]       # the dense agent projection
//! sealmap resolve  'sym:cargo shop . db/Db#insert().'   # current span and hashes
//! sealmap verify                                    # the seal gate over docs/diagrams (CI)
//! sealmap seal-check CP-03 CP-07                    # classify chosen lock entries
//! sealmap stale [--since main]                      # sealed symbols that changed; exit 0
//! sealmap seal sign CP-03 --reviewer zai:glm-5.3 --model claude:claude-sonnet-5-5   # write a seal (different families)
//! sealmap pack CP-03 CP-07 --budget 200000          # a review pack on stdout
//! sealmap pack --diff main --budget 200000 --shard -o packs/   # changed topics, sharded
//! ```
//!
//! Exit codes: 0 success; 1 a check failed, a symbol is not found, or a seal
//! was refused; 2 usage or IO error.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod review_cli;

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use clap::{Args, Parser, Subcommand};
use sealmap::corpus::pack::{self, PackError, PackInput, PackOptions};
use sealmap::corpus::seal::{self, Lock, Resolution, SealReport, Signature};
use sealmap::corpus::{self, Drift, ExternalLanes};
use sealmap::dense::{self, DenseOptions};
use sealmap::model::{Codebase, SourcePath, SymbolId};
use sealmap::rust::{ExternalCalls, RustOptions};
use sealmap::{Options, SourceSet};

#[derive(Parser)]
#[command(name = "sealmap", version, about = "Deterministic code maps and sealed diagram contracts")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate the 1:1 corpus, model and index into the output directory.
    Generate(Generate),
    /// Print the extracted model as JSON to stdout.
    Model(ModelArgs),
    /// Write the dense agent projection: dense.txt and _index.txt.
    Dense(DenseArgs),
    /// Where a symbol is now (span and hashes), or `absent` with rename candidates.
    Resolve(Resolve),
    /// Classify lock entries: holds, behaviour, contract, absent, ... Exits 1 unless all hold.
    SealCheck(SealCheck),
    /// The seal gate: seal-check every entry, plus pointer and lock faults. Exits 1 unless all hold.
    Verify(Corpus),
    /// Sealed symbols whose code changed, per topic. Always exits 0.
    Stale(Stale),
    /// Write seals.
    #[command(subcommand)]
    Seal(SealCmd),
    /// A deterministic review pack: topics, dense slices of the code they cite, source windows.
    Pack(PackArgs),
    /// Validate evidence-backed finding dispositions and separate delivery stages.
    #[command(subcommand)]
    Review(review_cli::Review),
}

#[derive(Subcommand)]
enum SealCmd {
    /// Derive a topic's seal from the current code and write it to the lock.
    Sign(Sign),
}

/// Where the code comes from and how it is read. Ids depend on these, so a
/// seal must be checked with the options it was signed with.
#[derive(Args, Clone)]
struct Source {
    /// Add a repository as NAME=PATH (repeatable) to analyse several together.
    #[arg(long = "repo", value_name = "NAME=PATH")]
    repos: Vec<String>,
    /// Codebase name (defaults to the directory or repo names).
    #[arg(long)]
    name: Option<String>,
    /// Include tests, examples and benches.
    #[arg(long)]
    tests: bool,
}

#[derive(Args)]
struct Generate {
    /// Source root (ignored when --repo is given).
    #[arg(default_value = ".")]
    path: PathBuf,
    #[command(flatten)]
    source: Source,
    /// Corpus directory.
    #[arg(short, long, default_value = ".sealmap")]
    out: PathBuf,
    /// Write nothing; compare the directory with a fresh generation and exit 1 on any drift.
    #[arg(long)]
    check: bool,
    /// External calls to keep in flows: all, non-std, none.
    #[arg(long, default_value = "non-std")]
    external: String,
    /// One sequence lane per external owner instead of per crate.
    #[arg(long)]
    owner_lanes: bool,
    /// Minimum calls for a function to get a sequence diagram.
    #[arg(long, default_value_t = 1)]
    min_calls: usize,
    /// Leave private items out of structure diagrams.
    #[arg(long)]
    public_only: bool,
    /// Do not write _model.json.
    #[arg(long)]
    no_model: bool,
    /// Pretty-print _index.json and _model.json.
    #[arg(long)]
    pretty: bool,
}

#[derive(Args)]
struct ModelArgs {
    /// Source root (ignored when --repo is given).
    #[arg(default_value = ".")]
    path: PathBuf,
    #[command(flatten)]
    source: Source,
    /// External calls to keep in flows: all, non-std, none.
    #[arg(long, default_value = "non-std")]
    external: String,
}

#[derive(Args)]
struct DenseArgs {
    /// Source root (ignored when --repo is given).
    #[arg(default_value = ".")]
    path: PathBuf,
    #[command(flatten)]
    source: Source,
    /// Output directory.
    #[arg(short, long, default_value = ".sealmap/dense")]
    out: PathBuf,
    /// Call levels expanded under a tree's root before a callee is cut.
    #[arg(long, default_value_t = DenseOptions::default().max_depth)]
    depth: usize,
    /// External calls to keep in flows: all, non-std, none.
    #[arg(long, default_value = "non-std")]
    external: String,
    /// Print sizes and call-edge counts to stderr.
    #[arg(long)]
    stats: bool,
}

/// Where the authored corpus and the code it cites live.
#[derive(Args)]
struct Site {
    /// Source root; also the base of a relative --diagrams.
    #[arg(long, short = 'C', default_value = ".")]
    root: PathBuf,
    /// The authored topic directory holding seals.lock.
    #[arg(long, default_value = "docs/diagrams")]
    diagrams: PathBuf,
    #[command(flatten)]
    source: Source,
}

/// The authored corpus and the code it cites, with report flags.
#[derive(Args)]
struct Corpus {
    #[command(flatten)]
    site: Site,
    /// Print JSON instead of text.
    #[arg(long)]
    json: bool,
    /// Also print findings that hold.
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
struct Resolve {
    /// The `sym:` id (quote it: ids contain spaces).
    id: String,
    #[command(flatten)]
    corpus: Corpus,
}

#[derive(Args)]
struct SealCheck {
    /// Topic ids to check (default: every lock entry).
    topics: Vec<String>,
    #[command(flatten)]
    corpus: Corpus,
}

#[derive(Args)]
struct Stale {
    /// Compare with this git revision instead of the lock's hashes.
    #[arg(long, value_name = "REV")]
    since: Option<String>,
    #[command(flatten)]
    corpus: Corpus,
}

#[derive(Args)]
struct PackArgs {
    /// Topic ids (`COR-04`) or topic files.
    topics: Vec<String>,
    /// Also pack every topic whose sealed or cited symbols changed since this git revision.
    #[arg(long, value_name = "REV")]
    diff: Option<String>,
    /// Refuse a pack longer than this many bytes (with --shard: per file).
    #[arg(long, value_name = "BYTES")]
    budget: Option<usize>,
    /// Call levels in each topic's dense slice.
    #[arg(long, default_value_t = PackOptions::default().depth)]
    depth: usize,
    /// Most source lines shown per cited symbol (0: none).
    #[arg(long, value_name = "LINES", default_value_t = PackOptions::default().source_window)]
    source_window: usize,
    /// Write the pack to this file instead of stdout (with --shard: a directory).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Split into numbered files of whole topics, each within --budget, instead of refusing.
    #[arg(long, requires_all = ["budget", "out"])]
    shard: bool,
    #[command(flatten)]
    site: Site,
    /// Print a refusal as JSON on stdout.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct Sign {
    /// Topic id (`COR-04`) or topic file path.
    topic: String,
    /// The one reviewer recorded in the seal (a single `host:model`, the model that gave the verdict; a multi-rung route is refused; must differ in family from --model).
    #[arg(long)]
    reviewer: String,
    /// Authoring model recorded in the seal (`host:model`, or `a -> b` if an escalation ran).
    #[arg(long)]
    model: String,
    /// Seal date, YYYY-MM-DD (default: today, UTC).
    #[arg(long)]
    date: Option<String>,
    #[command(flatten)]
    corpus: Corpus,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("sealmap: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.cmd {
        Cmd::Generate(g) => generate(&g),
        Cmd::Model(m) => {
            let (codebase, _) = extract(&m.path, &m.source, external(&m.external)?)?;
            println!("{}", serde_json::to_string_pretty(&codebase).map_err(|e| e.to_string())?);
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Dense(d) => dense(&d),
        Cmd::Resolve(r) => resolve(&r),
        Cmd::SealCheck(s) => {
            let only: BTreeSet<String> = s.topics.iter().cloned().collect();
            let ctx = Ctx::load(&s.corpus.site)?;
            let lock = match &ctx.lock_text {
                Some(text) => Lock::parse(text).map_err(|e| format!("{}: {e}", ctx.lock_path.display()))?,
                None => return Err(format!("{} does not exist", ctx.lock_path.display())),
            };
            let report = seal::seal_check(
                &lock,
                seal::LOCK_FILE,
                &ctx.topics,
                &ctx.codebase,
                (!only.is_empty()).then_some(&only),
            );
            print_report(&report, &s.corpus)
        }
        Cmd::Verify(c) => {
            let ctx = Ctx::load(&c.site)?;
            let report = seal::verify(ctx.lock_text.as_deref(), seal::LOCK_FILE, &ctx.topics, &ctx.codebase);
            print_report(&report, &c)
        }
        Cmd::Stale(s) => stale(&s),
        Cmd::Seal(SealCmd::Sign(s)) => sign(&s),
        Cmd::Pack(p) => pack(&p),
        Cmd::Review(r) => review_cli::run(r),
    }
}

// ------------------------------------------------------------------ generate

fn generate(g: &Generate) -> Result<ExitCode, String> {
    let mut opts = Options::default();
    opts.corpus.min_calls = g.min_calls;
    opts.corpus.include_private = !g.public_only;
    opts.corpus.emit_model = !g.no_model;
    opts.corpus.pretty_json = g.pretty;
    if g.owner_lanes {
        opts.corpus.external_lanes = ExternalLanes::Owner;
    }
    let (codebase, _) = extract(&g.path, &g.source, external(&g.external)?)?;
    let corpus = sealmap::generate(&codebase, &opts.corpus);
    if g.check {
        let report = corpus::verify(&g.out, &corpus).map_err(|e| e.to_string())?;
        for e in &report.entries {
            println!("{:<9} {}", e.drift.to_string(), e.path);
        }
        if report.is_clean() {
            eprintln!("sealmap: {} matches a fresh generation ({} files)", g.out.display(), report.checked);
            return Ok(ExitCode::SUCCESS);
        }
        eprintln!("sealmap: {} files differ from a fresh generation", report.entries.len());
        return Ok(ExitCode::from(1));
    }
    let report = corpus::write(&g.out, &corpus).map_err(|e| e.to_string())?;
    let stats = codebase.stats();
    let removed = report.count(Drift::Orphaned);
    eprintln!(
        "sealmap: {} files, {} symbols, {} sequences → {} ({} written, {} removed, {} unchanged)",
        stats.files,
        stats.symbols,
        corpus.index.fragments().filter(|f| f.kind == corpus::FragmentKind::Sequence).count(),
        g.out.display(),
        report.entries.len() - removed,
        removed,
        report.checked - (report.entries.len() - removed),
    );
    Ok(ExitCode::SUCCESS)
}

// ------------------------------------------------------------------ dense

fn dense(d: &DenseArgs) -> Result<ExitCode, String> {
    let (cb, _, sources) = extract_sources(&d.path, &d.source, external(&d.external)?)?;
    let out = dense::render(&cb, &DenseOptions::with_max_depth(d.depth));
    fs::create_dir_all(&d.out).map_err(|e| format!("{}: {e}", d.out.display()))?;
    for (name, text) in out.files() {
        let path = d.out.join(name);
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if d.stats {
        let source: usize = cb.files.keys().filter_map(|p| sources.get(p)).map(str::len).sum();
        let calls_at = out.text.find("\n# calls\n").map_or(out.text.len(), |i| i + 1);
        let stats = cb.stats();
        eprintln!(
            "source={source} dense={} index={} skeleton={} calls={} call_edges={} symbols={} ratio={:.3}",
            out.text.len(),
            out.index.len(),
            calls_at,
            out.text.len() - calls_at,
            stats.calls,
            stats.symbols,
            out.text.len() as f64 / source.max(1) as f64,
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn external(s: &str) -> Result<ExternalCalls, String> {
    match s {
        "all" => Ok(ExternalCalls::All),
        "non-std" => Ok(ExternalCalls::NonStd),
        "none" => Ok(ExternalCalls::None),
        other => Err(format!("--external must be all, non-std or none (got `{other}`)")),
    }
}

// ------------------------------------------------------------------ loading

/// Extract the codebase at `root` (or the `--repo` set). Returns the model
/// and the codebase name used, so a second tree can be read under the same
/// name.
fn extract(root: &Path, source: &Source, external_calls: ExternalCalls) -> Result<(Codebase, String), String> {
    extract_sources(root, source, external_calls).map(|(cb, name, _)| (cb, name))
}

/// [`extract`], also returning the source texts that were read.
fn extract_sources(
    root: &Path,
    source: &Source,
    external_calls: ExternalCalls,
) -> Result<(Codebase, String, SourceSet), String> {
    let mut ro = RustOptions { include_tests: source.tests, external_calls, ..RustOptions::default() };
    let sources: SourceSet = if source.repos.is_empty() {
        ro.name = source.name.clone().unwrap_or_else(|| dir_name(root));
        sealmap::rust::load_dir(root).map_err(|e| format!("{}: {e}", root.display()))?
    } else {
        let repos = parse_repos(&source.repos)?;
        ro.name =
            source.name.clone().unwrap_or_else(|| repos.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join("+"));
        let refs: Vec<(&str, &Path)> = repos.iter().map(|(n, p)| (n.as_str(), p.as_path())).collect();
        sealmap::load_repos(&refs).map_err(|e| e.to_string())?
    };
    let extraction = sealmap::rust::extract(&sources, &ro);
    for d in &extraction.diagnostics {
        eprintln!("warning: {}: {}", d.file, d.message);
    }
    Ok((extraction.codebase, ro.name, sources))
}

fn parse_repos(repos: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    repos
        .iter()
        .map(|r| {
            let (name, path) = r.split_once('=').ok_or_else(|| format!("--repo expects NAME=PATH, got `{r}`"))?;
            Ok((name.to_owned(), PathBuf::from(path)))
        })
        .collect()
}

fn dir_name(p: &Path) -> String {
    p.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "codebase".into())
}

/// Everything the seal commands read.
struct Ctx {
    codebase: Codebase,
    sources: SourceSet,
    name: String,
    diagrams: PathBuf,
    lock_path: PathBuf,
    lock_text: Option<String>,
    topics: seal::Topics,
}

impl Ctx {
    fn load(c: &Site) -> Result<Self, String> {
        let (codebase, name, sources) = extract_sources(&c.root, &c.source, ExternalCalls::NonStd)?;
        let diagrams = if c.diagrams.is_absolute() { c.diagrams.clone() } else { c.root.join(&c.diagrams) };
        let lock_path = diagrams.join(seal::LOCK_FILE);
        let lock_text = match fs::read_to_string(&lock_path) {
            Ok(t) => Some(t),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("{}: {e}", lock_path.display())),
        };
        let topics = read_topics(&diagrams).map_err(|e| format!("{}: {e}", diagrams.display()))?;
        Ok(Self { codebase, sources, name, diagrams, lock_path, lock_text, topics })
    }
    /// The lock, if there is one.
    fn lock(&self) -> Result<Option<Lock>, String> {
        self.lock_text
            .as_deref()
            .map(|text| Lock::parse(text).map_err(|e| format!("{}: {e}", self.lock_path.display())))
            .transpose()
    }

    /// The model of the code at git revision `rev`, read under the same name
    /// and options as the current one. `flag` names the option for errors.
    fn model_at(&self, site: &Site, rev: &str, flag: &str) -> Result<Codebase, String> {
        if !site.source.repos.is_empty() {
            return Err(format!("{flag} reads one git repository; it cannot be combined with --repo"));
        }
        let tree = GitTree::export(&site.root, rev)?;
        let source = Source { name: Some(self.name.clone()), ..site.source.clone() };
        Ok(extract(tree.path(), &source, ExternalCalls::NonStd)?.0)
    }
}

/// Every `.md` file under `dir` with a front-matter `id:`, keyed by path
/// relative to `dir`. Hidden directories are skipped.
fn read_topics(dir: &Path) -> io::Result<seal::Topics> {
    let mut out = seal::Topics::new();
    if dir.is_dir() {
        read_topics_rec(dir, dir, &mut out)?;
    }
    Ok(out)
}

fn read_topics_rec(root: &Path, dir: &Path, out: &mut seal::Topics) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let ty = e.file_type()?;
        if ty.is_dir() {
            read_topics_rec(root, &path, out)?;
        } else if ty.is_file() && path.extension().is_some_and(|x| x == "md") {
            let text = fs::read_to_string(&path)
                .map_err(|err| io::Error::new(err.kind(), format!("{}: {err}", path.display())))?;
            if seal::topic_id(&text).is_some() {
                let rel = SourcePath::relative_to(&path, root)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
                out.insert(rel, text);
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ resolve

fn resolve(r: &Resolve) -> Result<ExitCode, String> {
    let id = SymbolId::parse(&r.id).map_err(|e| format!("`{}` is not a sym: id: {e}", r.id))?;
    let ctx = Ctx::load(&r.corpus.site)?;
    // The sealed body, from any entry that seals the id, gives rename candidates.
    let sealed_body = match &ctx.lock_text {
        Some(text) => Lock::parse(text)
            .map_err(|e| format!("{}: {e}", ctx.lock_path.display()))?
            .topics
            .values()
            .find_map(|t| t.symbols.get(&id).map(|s| s.body)),
        None => None,
    };
    let res = seal::resolve(&ctx.codebase, &id, sealed_body);
    if r.corpus.json {
        let v = serde_json::json!({ "id": id, "resolution": res });
        println!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
    } else {
        match &res {
            Resolution::Found { file, span, sig, body } => {
                println!("found {file}:{}-{}", span.start_line, span.end_line);
                println!("sig   {sig}");
                println!("body  {body}");
            }
            Resolution::Unparsable { file } => println!("unparsable {file}"),
            Resolution::Absent { candidates } => {
                println!("absent");
                for c in candidates {
                    println!("candidate {c}");
                }
            }
        }
    }
    Ok(if matches!(res, Resolution::Found { .. }) { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

// ------------------------------------------------------------------ reports

fn print_report(report: &SealReport, c: &Corpus) -> Result<ExitCode, String> {
    if c.json {
        println!("{}", serde_json::to_string_pretty(report).map_err(|e| e.to_string())?);
    } else {
        for f in report.findings.iter().filter(|f| c.all || !f.class.passes()) {
            println!(
                "{:<17} {:<8} {} {}",
                f.class.name(),
                f.topic.as_deref().unwrap_or("-"),
                f.symbol.as_deref().unwrap_or("-"),
                f.detail
            );
            for cand in &f.candidates {
                println!("{:<17} {:<8} {cand}", "  candidate", "");
            }
        }
    }
    let failed = report.failures().count();
    let topics: BTreeSet<_> = report.findings.iter().filter_map(|f| f.topic.as_deref()).collect();
    eprintln!(
        "sealmap: {} topic(s) checked, {} finding(s) fail, {} hold; {} topic(s) carry no seal and no citation",
        topics.len(),
        failed,
        report.count(seal::Class::Holds),
        report.unsealed_topics.len()
    );
    Ok(if report.passes() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

// ------------------------------------------------------------------ stale

fn stale(s: &Stale) -> Result<ExitCode, String> {
    let ctx = Ctx::load(&s.corpus.site)?;
    let lock = ctx.lock()?.unwrap_or_default();
    let before = match &s.since {
        None => None,
        Some(rev) => Some(ctx.model_at(&s.corpus.site, rev, "--since")?),
    };
    let changed = seal::stale(&lock, before.as_ref(), &ctx.codebase);
    if s.corpus.json {
        println!("{}", serde_json::to_string_pretty(&changed).map_err(|e| e.to_string())?);
    } else {
        for c in &changed {
            println!("{:<8} {:<10} {} {}", c.topic, c.class.name(), c.symbol, c.detail);
            for cand in &c.candidates {
                println!("{:<8} {:<10} {cand}", "", "  candidate");
            }
        }
    }
    let topics: BTreeSet<_> = changed.iter().map(|c| c.topic.as_str()).collect();
    eprintln!(
        "sealmap: {} of {} sealed topic(s) have changed symbols (since {})",
        topics.len(),
        lock.topics.len(),
        s.since.as_deref().unwrap_or("sealing")
    );
    Ok(ExitCode::SUCCESS)
}

/// A git revision's tree exported into a temporary directory with
/// `git archive | tar -x`. Removed on drop. Only the subdirectory of the
/// repository that `root` points at is exported, so paths match `root`.
struct GitTree(PathBuf);

impl GitTree {
    fn export(root: &Path, rev: &str) -> Result<Self, String> {
        let prefix = git(root, &["rev-parse", "--show-prefix"])?;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("sealmap-since-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tree = Self(dir);
        let mut archive = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["archive", "--format=tar", &format!("{rev}:{}", prefix.trim())])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("git archive: {e}"))?;
        let stdout = archive.stdout.take().ok_or("git archive: no stdout")?;
        let untar = Command::new("tar")
            .arg("-x")
            .arg("-C")
            .arg(&tree.0)
            .stdin(Stdio::from(stdout))
            .status()
            .map_err(|e| format!("tar: {e}"))?;
        let archived = archive.wait().map_err(|e| format!("git archive: {e}"))?;
        if !archived.success() {
            return Err(format!("git archive {rev} failed ({archived})"));
        }
        if !untar.success() {
            return Err(format!("tar -x failed ({untar})"));
        }
        Ok(tree)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for GitTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("git {}: {e}", args.join(" ")))
}

// ------------------------------------------------------------------ sign

fn sign(s: &Sign) -> Result<ExitCode, String> {
    let ctx = Ctx::load(&s.corpus.site)?;
    // A parsable but non-canonical lock is rewritten canonically.
    let mut lock = ctx.lock()?.unwrap_or_default();
    let file = topic_file(&ctx, &s.topic)?;
    let text = &ctx.topics[&file];
    let who = Signature {
        reviewer: s.reviewer.clone(),
        model: s.model.clone(),
        date: s.date.clone().unwrap_or_else(today_utc),
    };
    let sealed_text = match seal::sign(&mut lock, &file, text, &ctx.codebase, seal::LOCK_FILE, &who) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("sealmap: not sealed: {e}");
            return Ok(ExitCode::from(1));
        }
    };
    let topic_path = ctx.diagrams.join(file.as_str());
    if &sealed_text != text {
        fs::write(&topic_path, &sealed_text).map_err(|e| format!("{}: {e}", topic_path.display()))?;
    }
    fs::write(&ctx.lock_path, lock.to_toml()).map_err(|e| format!("{}: {e}", ctx.lock_path.display()))?;
    let entry = lock.by_file(&file).expect("sign inserted the entry");
    eprintln!(
        "sealmap: sealed {} ({}) with {} symbol(s) into {}",
        entry.id,
        file,
        entry.symbols.len(),
        ctx.lock_path.display()
    );
    Ok(ExitCode::SUCCESS)
}

// ------------------------------------------------------------------ pack

fn pack(p: &PackArgs) -> Result<ExitCode, String> {
    let ctx = Ctx::load(&p.site)?;
    let mut ids = BTreeSet::new();
    for arg in &p.topics {
        let file = topic_file(&ctx, arg)?;
        let id = seal::topic_id(&ctx.topics[&file]).ok_or_else(|| format!("{file} has no front-matter id"))?;
        ids.insert(id);
    }
    if let Some(rev) = &p.diff {
        let before = ctx.model_at(&p.site, rev, "--diff")?;
        let changed = pack::changed_topics(ctx.lock()?.as_ref(), &ctx.topics, &before, &ctx.codebase);
        eprintln!("sealmap: {} topic(s) changed since {rev}", changed.len());
        ids.extend(changed);
    } else if ids.is_empty() {
        return Err("name the topics to pack, or select them with --diff REV".into());
    }
    if ids.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    let ids: Vec<String> = ids.into_iter().collect();
    let revision = revision(&p.site.root);
    let mut input = PackInput::new(&ctx.codebase, &ctx.sources, &ctx.topics, &revision);
    if let Some(rev) = &p.diff {
        input = input.diff(rev);
    }
    let mut options = PackOptions::default().depth(p.depth).source_window(p.source_window);
    if let Some(b) = p.budget {
        options = options.budget(b);
    }
    let result = if p.shard {
        pack::shard(&input, &ids, &options)
    } else {
        pack::pack(&input, &ids, &options).map(|one| vec![one])
    };
    let packs = match result {
        Ok(packs) => packs,
        Err(e) => {
            if p.json {
                println!("{}", serde_json::to_string_pretty(&e).map_err(|e| e.to_string())?);
            }
            eprintln!("sealmap: not packed: {e}");
            if let PackError::OverBudget { header, topics, .. } = &e {
                eprintln!("{:<10} {header:>10}", "header");
                for t in topics {
                    eprintln!("{:<10} {:>10}", t.id, t.bytes);
                }
            }
            return Ok(ExitCode::from(1));
        }
    };
    match (&p.out, p.shard) {
        (None, _) => print!("{}", packs[0].text),
        (Some(file), false) => fs::write(file, &packs[0].text).map_err(|e| format!("{}: {e}", file.display()))?,
        (Some(dir), true) => {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let width = packs.len().to_string().len().max(2);
            for (i, one) in packs.iter().enumerate() {
                let file = dir.join(format!("pack-{:0width$}.txt", i + 1));
                fs::write(&file, &one.text).map_err(|e| format!("{}: {e}", file.display()))?;
            }
        }
    }
    let bytes: usize = packs.iter().map(|one| one.text.len()).sum();
    eprintln!("sealmap: packed {} topic(s) into {} pack(s), {bytes} bytes", ids.len(), packs.len());
    Ok(ExitCode::SUCCESS)
}

/// The checkout's revision for a pack header: `HEAD`'s commit, with
/// `+dirty` when tracked files differ from it, or `unknown` outside git.
/// Untracked files are not consulted, so writing a pack into the checkout
/// does not change the next pack's header.
fn revision(root: &Path) -> String {
    let Ok(head) = git(root, &["rev-parse", "HEAD"]) else { return "unknown".into() };
    let dirty = git(root, &["status", "--porcelain", "--untracked-files=no"]).is_ok_and(|s| !s.trim().is_empty());
    format!("{}{}", head.trim(), if dirty { "+dirty" } else { "" })
}

/// A topic named by id, or by a path relative to the current directory or
/// to the diagrams directory.
fn topic_file(ctx: &Ctx, arg: &str) -> Result<SourcePath, String> {
    if let Some((path, _)) = ctx.topics.iter().find(|(_, t)| seal::topic_id(t).as_deref() == Some(arg)) {
        return Ok(path.clone());
    }
    let p = Path::new(arg);
    let candidates = [p.canonicalize().ok(), ctx.diagrams.join(p).canonicalize().ok()];
    let diagrams = ctx.diagrams.canonicalize().map_err(|e| format!("{}: {e}", ctx.diagrams.display()))?;
    for c in candidates.into_iter().flatten() {
        if let Ok(rel) = SourcePath::relative_to(&c, &diagrams) {
            if ctx.topics.contains_key(&rel) {
                return Ok(rel);
            }
        }
    }
    Err(format!("`{arg}` is neither a topic id nor a topic file under {}", ctx.diagrams.display()))
}

/// Today's date in UTC, `YYYY-MM-DD`.
fn today_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    civil_date((secs / 86_400) as i64)
}

/// The proleptic Gregorian date `days` after 1970-01-01 (Hinnant's
/// civil-from-days).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::civil_date;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(civil_date(19_723), "2024-01-01");
        assert_eq!(civil_date(20_731), "2026-10-05");
        assert_eq!(civil_date(-1), "1969-12-31");
    }
}
