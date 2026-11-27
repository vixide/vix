//! Secret scanning (T832, `spec/test` "Repository invariants"): no tracked
//! file holds a credential that looks real. It reads every file `git ls-files`
//! lists, in the working tree only (no history, no network, no external
//! scanner), and fails on a high-confidence match of one of [`RULES`]:
//!
//! - a private key header (`-----BEGIN … PRIVATE KEY-----`, PEM, OpenSSH or
//!   PGP);
//! - an AWS access key id (`AKIA` and 16 upper-case letters or digits);
//! - a GitHub token: a classic one (`ghp_`, `gho_`, `ghu_`, `ghs_` or `ghr_`
//!   and 36 letters or digits) or a fine-grained one (`github_pat_` and 82
//!   letters, digits or `_`);
//! - a Slack token (`xoxb-`, `xoxa-`, `xoxp-`, `xoxr-` or `xoxs-`, then the
//!   numeric team or user id and the secret part);
//! - an Anthropic API key (`sk-ant-` and at least 32 key characters);
//! - an OpenAI-style key (`sk-` and at least 40 key characters, at least one
//!   a digit and one a letter).
//!
//! "High confidence" means the shape of a real credential, with its real
//! length, not a mention: `sk-ant-test`, `xoxb-your-token` or a `TOKEN=secret`
//! line in a test fixture are not matches. The patterns are spelled in pieces
//! here, and the self-tests build their fake secrets at run time, so this file
//! does not match itself.
//!
//! A file that deliberately holds a fake secret goes in [`ALLOWED`] with the
//! reason; the test fails on an entry that no longer matches, so the list
//! cannot go stale. Binary files (a NUL in the first 8 KiB) and files over
//! 16 MiB are skipped. Without `git`, or outside a git checkout (a crates.io
//! package), the scan is skipped with a note on stderr.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use regex::bytes::Regex;

/// `(repository-relative path, why it may hold a secret-shaped string)`.
///
/// Empty: the working tree holds no fixture that matches a rule. The fake
/// secrets the tests do use are deliberately too short to match:
/// `crates/vix-ai-core/src/secret.rs` and `crates/vix-ai-core/src/anthropic.rs`
/// use `sk-ant-test` and `sk-ant-abc`, and `crates/vix-mcp-server/src/tests.rs`
/// writes a `.env` holding `TOKEN=secret`. A fixture that needs a full-length
/// fake belongs here, with its reason.
const ALLOWED: &[(&str, &str)] = &[];

/// Bytes read to decide whether a file is binary.
const BINARY_SNIFF: usize = 8 * 1024;

/// Files larger than this are not read (none of the tracked text files is
/// near it; the largest, a locale catalog, is under 2 MiB).
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// One kind of secret: a name and a pattern whose group 1 is the secret.
struct Rule {
    /// What a match is, for the failure message.
    name: &'static str,
    /// The pattern; group 1 is the secret, and anything before it is the
    /// boundary that keeps a match from starting inside a longer word.
    pattern: Regex,
    /// An extra check on the secret, for a pattern a regex alone would make
    /// too loose.
    accept: fn(&[u8]) -> bool,
}

/// Accepts every match.
fn any(_: &[u8]) -> bool {
    true
}

/// Whether the part after `sk-` holds both a digit and a letter, as a random
/// key does and a long kebab-case word does not.
fn random_looking(secret: &[u8]) -> bool {
    let body = secret.get(3..).unwrap_or_default();
    body.iter().any(u8::is_ascii_digit) && body.iter().any(u8::is_ascii_alphabetic)
}

/// The rules, compiled. Each pattern is joined from pieces so that this source
/// file never holds a contiguous match.
fn rules() -> Vec<Rule> {
    let rule = |name, pattern: String, accept| {
        Regex::new(&pattern)
            .map(|pattern| Rule {
                name,
                pattern,
                accept,
            })
            .map_err(|e| format!("{name}: {e}"))
    };
    let dashes = "-".repeat(5);
    [
        rule(
            "private key",
            format!("()({dashes}BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?{dashes})"),
            any,
        ),
        rule(
            "AWS access key id",
            format!("(?:^|[^A-Z0-9])({}[0-9A-Z]{{16}})(?:$|[^A-Z0-9])", "AK".to_owned() + "IA"),
            any,
        ),
        rule(
            "GitHub token",
            format!(
                "(?:^|[^A-Za-z0-9_])((?:gh[pousr]{u}[A-Za-z0-9]{{36}})|(?:github{u}pat{u}[A-Za-z0-9_]{{82}}))(?:$|[^A-Za-z0-9_])",
                u = "_"
            ),
            any,
        ),
        rule(
            "Slack token",
            format!("(?:^|[^A-Za-z0-9])({}[baprs]-[0-9]{{6,}}-[0-9A-Za-z-]{{8,}})", "xo".to_owned() + "x"),
            any,
        ),
        rule(
            "Anthropic API key",
            format!("(?:^|[^A-Za-z0-9_-])({}-ant-[A-Za-z0-9_-]{{32,}})", "s".to_owned() + "k"),
            any,
        ),
        rule(
            "OpenAI-style API key",
            format!("(?:^|[^A-Za-z0-9_-])({}-[A-Za-z0-9_-]{{40,}})", "s".to_owned() + "k"),
            random_looking,
        ),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .unwrap_or_else(|e| panic!("a secret rule does not compile: {e}"))
}

/// `(1-based line, rule name)` for every secret in `text`, one per position
/// (an Anthropic key is not reported a second time as an OpenAI-style one).
fn scan(rules: &[Rule], text: &[u8]) -> Vec<(usize, &'static str)> {
    let mut hits: Vec<(usize, &'static str)> = Vec::new();
    let mut starts = Vec::new();
    for rule in rules {
        for caps in rule.pattern.captures_iter(text) {
            let Some(secret) = caps.get(1) else {
                continue;
            };
            if starts.contains(&secret.start()) || !(rule.accept)(secret.as_bytes()) {
                continue;
            }
            starts.push(secret.start());
            // One more segment than there are line breaks before the match.
            let line = text
                .get(..secret.start())
                .map_or(1, |before| before.split(|&b| b == b'\n').count());
            hits.push((line, rule.name));
        }
    }
    hits.sort_unstable();
    hits
}

/// The repository root (this test's package lives in `tui/`).
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
}

/// The tracked files, repository-relative, or `None` without git or outside
/// a checkout.
fn tracked_files(root: &Path) -> Option<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        out.stdout
            .split(|&b| b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect(),
    )
}

/// The file's bytes, or `None` for a missing, unreadable, oversized or binary
/// file (or a submodule, which `git ls-files` lists as a path).
fn text_of(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let head = bytes
        .get(..BINARY_SNIFF.min(bytes.len()))
        .unwrap_or_default();
    (!head.contains(&0)).then_some(bytes)
}

#[test]
fn no_tracked_file_holds_a_secret() {
    let root = root();
    let Some(files) = tracked_files(root) else {
        eprintln!("secret_scan: skipped, `git ls-files` is not available here");
        return;
    };
    let started = Instant::now();
    let rules = rules();
    let mut found = Vec::new();
    let mut matched_allowed = Vec::new();
    let mut scanned = 0_usize;
    for rel in &files {
        let Some(text) = text_of(&root.join(rel)) else {
            continue;
        };
        scanned = scanned.saturating_add(1);
        let hits = scan(&rules, &text);
        if hits.is_empty() {
            continue;
        }
        if ALLOWED.iter().any(|(path, _)| path == rel) {
            matched_allowed.push(rel.clone());
            continue;
        }
        for (line, name) in hits {
            found.push(format!("{rel}:{line}: {name}"));
        }
    }
    eprintln!(
        "secret_scan: {scanned} of {} tracked files in {:?}",
        files.len(),
        started.elapsed()
    );
    assert!(
        found.is_empty(),
        "tracked files hold what looks like a real secret. Remove it (and revoke it \
         if it is real); a deliberate fake belongs in ALLOWED in tui/tests/secret_scan.rs \
         with its reason:\n{}",
        found.join("\n")
    );
    let stale: Vec<_> = ALLOWED
        .iter()
        .filter(|(path, _)| !matched_allowed.iter().any(|m| m == path))
        .map(|(path, _)| *path)
        .collect();
    assert!(
        stale.is_empty(),
        "ALLOWED names files that no longer hold a secret-shaped string; remove them: {stale:?}"
    );
}

/// The rule names `text` matches, in line order.
fn names(text: &str) -> Vec<&'static str> {
    scan(&rules(), text.as_bytes())
        .into_iter()
        .map(|(_, name)| name)
        .collect()
}

#[test]
fn each_rule_finds_a_real_shaped_secret() {
    let dashes = "-".repeat(5);
    let key = format!("{dashes}BEGIN OPENSSH PRIVATE KEY{dashes}\nb3BlbnNzaC1rZXk=\n");
    assert_eq!(names(&key), ["private key"]);
    let pem = format!("x\n{dashes}BEGIN RSA PRIVATE KEY{dashes}\n");
    assert_eq!(scan(&rules(), pem.as_bytes()), [(2, "private key")]);
    let pgp = format!("{dashes}BEGIN PGP PRIVATE KEY BLOCK{dashes}");
    assert_eq!(names(&pgp), ["private key"]);

    let aws = format!("aws_access_key_id = {}{}", "AKIA", "IOSFODNN7EXAMPLE");
    assert_eq!(names(&aws), ["AWS access key id"]);

    let classic = format!("token: {}_{}", "ghp", "a1B2".repeat(9));
    assert_eq!(names(&classic), ["GitHub token"]);
    let fine = format!("\"{}_pat_{}\"", "github", "A1_b".repeat(20) + "c2");
    assert_eq!(names(&fine), ["GitHub token"]);

    let slack = format!("{}b-123456789012-1234567890123-AbCdEfGhIjKl", "xox");
    assert_eq!(names(&slack), ["Slack token"]);

    let anthropic = format!("{}-ant-api03-{}", "sk", "Ab1-".repeat(10));
    assert_eq!(names(&anthropic), ["Anthropic API key"]);
    let openai = format!("OPENAI_API_KEY={}-proj-{}", "sk", "a1B2c3".repeat(8));
    assert_eq!(names(&openai), ["OpenAI-style API key"]);
}

#[test]
fn mentions_and_short_fakes_are_not_secrets() {
    for text in [
        "a header line: BEGIN RSA PRIVATE KEY, written without its dashes",
        "\"sk-ant-test\" and sk-ant-abc are the fixtures vix-ai-core uses",
        "set it to xoxb-your-token-here",
        "TOKEN=secret",
        "the AKIA prefix marks a long-term key; AKIA1234 is too short",
        "ghp_ followed by 36 characters",
        "risk-assessment-for-the-new-feature-flag-rollout-plan-2026",
        "task-0123456789-abcdefghij-0123456789-abcdefghij",
        "sk-this-is-a-long-kebab-case-name-without-any-numbers-at-all",
    ] {
        assert_eq!(names(text), Vec::<&str>::new(), "{text}");
    }
    // A longer run of key characters around a classic token is not a token.
    let embedded = format!("x{}_{}y9", "ghp", "a1B2".repeat(9));
    assert_eq!(names(&embedded), Vec::<&str>::new());
}
