//! Secret scanning (T832, `spec/test` "Repository invariants"): no tracked
//! file holds a credential that looks real. It reads every file `git ls-files`
//! lists, in the working tree only (no history, no network, no external
//! scanner), and fails on a high-confidence match of one of the rules in
//! [`PATTERNS`]:
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
//! line in a test fixture are not matches. The patterns are spelled so the
//! pattern file does not match itself, and the self-tests build their fake
//! secrets at run time, so this file does not either.
//!
//! The rules and the allowlist live in `scripts/secret-patterns`
//! ([`PATTERNS`]), which `scripts/hooks/pre-commit` also reads to scan staged
//! files before a commit (T833), so the two cannot drift. A file that
//! deliberately holds a fake secret goes on an `allow` line there with the
//! reason; the test fails on an entry that no longer matches, so the list
//! cannot go stale. Binary files (a NUL in the first 8 KiB) and files over
//! 16 MiB are skipped. Without `git`, or outside a git checkout (a crates.io
//! package), the scan is skipped with a note on stderr.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use regex::bytes::Regex;

/// The rule and allowlist file, repository-relative. It is shared with
/// `scripts/hooks/pre-commit`, which scans staged files with the same rules
/// through `grep -E`, so the file's syntax is the part of a POSIX extended
/// regular expression the `regex` crate reads the same way.
const PATTERNS: &str = "scripts/secret-patterns";

/// Bytes read to decide whether a file is binary.
const BINARY_SNIFF: usize = 8 * 1024;

/// Files larger than this are not read (none of the tracked text files is
/// near it; the largest, a locale catalog, is under 2 MiB).
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// One kind of secret: a name and a pattern whose group 1 is the secret.
struct Rule {
    /// What a match is, for the failure message.
    name: String,
    /// The pattern; group 1 is the secret, and anything before it is the
    /// boundary that keeps a match from starting inside a longer word.
    pattern: Regex,
    /// An extra check on the secret, for a pattern a regex alone would make
    /// too loose.
    accept: fn(&[u8]) -> bool,
}

/// The rules and the allowlist read from [`PATTERNS`].
struct Patterns {
    /// The compiled `rule` lines, in file order.
    rules: Vec<Rule>,
    /// `(repository-relative path, why it may hold a secret-shaped string)`,
    /// from the `allow` lines.
    allowed: Vec<(String, String)>,
}

/// Accepts every match.
fn any(_: &[u8]) -> bool {
    true
}

/// Whether the part after the secret's first `-` holds both a digit and a
/// letter, as a random key does and a long kebab-case word does not.
fn random_looking(secret: &[u8]) -> bool {
    let body = secret
        .iter()
        .position(|&b| b == b'-')
        .and_then(|dash| secret.get(dash.saturating_add(1)..))
        .unwrap_or_default();
    body.iter().any(u8::is_ascii_digit) && body.iter().any(u8::is_ascii_alphabetic)
}

/// Parses the text of [`PATTERNS`]; the error names the bad line.
fn parse_patterns(text: &str) -> Result<Patterns, String> {
    let mut rules = Vec::new();
    let mut allowed = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index.saturating_add(1);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["rule", name, accept, before, secret, after] => {
                let accept: fn(&[u8]) -> bool = match *accept {
                    "any" => any,
                    "random" => random_looking,
                    other => return Err(format!("line {number}: unknown accept `{other}`")),
                };
                let before = if *before == "-" {
                    String::new()
                } else {
                    format!("(?:^|{before})")
                };
                let after = if *after == "-" {
                    String::new()
                } else {
                    format!("(?:$|{after})")
                };
                let pattern = format!("{before}({secret}){after}");
                let pattern = Regex::new(&pattern).map_err(|e| format!("line {number}: {e}"))?;
                rules.push(Rule {
                    name: (*name).to_owned(),
                    pattern,
                    accept,
                });
            }
            ["allow", path, reason] if !path.is_empty() && !reason.is_empty() => {
                allowed.push(((*path).to_owned(), (*reason).to_owned()));
            }
            _ => return Err(format!("line {number}: not a rule or allow line: {line}")),
        }
    }
    if rules.is_empty() {
        return Err("no rule lines".to_owned());
    }
    Ok(Patterns { rules, allowed })
}

/// The rules and allowlist, or `None` (with a note on stderr) when
/// [`PATTERNS`] is not there, as in a crates.io package.
fn patterns() -> Option<Patterns> {
    let Ok(text) = std::fs::read_to_string(root().join(PATTERNS)) else {
        eprintln!("secret_scan: skipped, {PATTERNS} is not here");
        return None;
    };
    Some(parse_patterns(&text).unwrap_or_else(|e| panic!("{PATTERNS}: {e}")))
}

/// `(1-based line, rule name)` for every secret in `text`, one per position
/// (an Anthropic key is not reported a second time as an OpenAI-style one).
fn scan<'r>(rules: &'r [Rule], text: &[u8]) -> Vec<(usize, &'r str)> {
    let mut hits: Vec<(usize, &'r str)> = Vec::new();
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
            hits.push((line, rule.name.as_str()));
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
    let Some(patterns) = patterns() else {
        return;
    };
    let started = Instant::now();
    let mut found = Vec::new();
    let mut matched_allowed = Vec::new();
    let mut scanned = 0_usize;
    for rel in &files {
        let Some(text) = text_of(&root.join(rel)) else {
            continue;
        };
        scanned = scanned.saturating_add(1);
        let hits = scan(&patterns.rules, &text);
        if hits.is_empty() {
            continue;
        }
        if patterns.allowed.iter().any(|(path, _)| path == rel) {
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
         if it is real); a deliberate fake belongs on an `allow` line in scripts/secret-patterns \
         with its reason:\n{}",
        found.join("\n")
    );
    let stale: Vec<_> = patterns
        .allowed
        .iter()
        .filter(|(path, _)| !matched_allowed.iter().any(|m| m == path))
        .map(|(path, _)| path.as_str())
        .collect();
    assert!(
        stale.is_empty(),
        "{PATTERNS} allows files that no longer hold a secret-shaped string; remove them: {stale:?}"
    );
}

/// The rule names `text` matches, in line order.
fn names(patterns: &Patterns, text: &str) -> Vec<String> {
    scan(&patterns.rules, text.as_bytes())
        .into_iter()
        .map(|(_, name)| name.to_owned())
        .collect()
}

#[test]
fn each_rule_finds_a_real_shaped_secret() {
    let Some(p) = patterns() else {
        return;
    };
    let names = |text: &str| names(&p, text);
    let dashes = "-".repeat(5);
    let key = format!("{dashes}BEGIN OPENSSH PRIVATE KEY{dashes}\nb3BlbnNzaC1rZXk=\n");
    assert_eq!(names(&key), ["private key"]);
    let pem = format!("x\n{dashes}BEGIN RSA PRIVATE KEY{dashes}\n");
    assert_eq!(scan(&p.rules, pem.as_bytes()), [(2, "private key")]);
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
    let Some(p) = patterns() else {
        return;
    };
    let names = |text: &str| names(&p, text);
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
        assert_eq!(names(text), Vec::<String>::new(), "{text}");
    }
    // A longer run of key characters around a classic token is not a token.
    let embedded = format!("x{}_{}y9", "ghp", "a1B2".repeat(9));
    assert_eq!(names(&embedded), Vec::<String>::new());
}

#[test]
fn the_pattern_file_parses_and_rejects_bad_lines() {
    let Some(p) = patterns() else {
        return;
    };
    assert_eq!(
        p.rules.len(),
        6,
        "a rule was added or lost: update the module docs"
    );
    let good = "# c\n\nrule\tx\tany\t-\tab+c\t-\nallow\tsome/file\tits reason\n";
    let parsed = parse_patterns(good).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(parsed.rules.len(), 1);
    assert_eq!(
        parsed.allowed,
        [("some/file".to_owned(), "its reason".to_owned())]
    );
    for bad in [
        "rule\tx\tsometimes\t-\tab\t-",
        "rule\tx\tany\t-\t(ab\t-",
        "rule\tx\tany\tab",
        "allow\tsome/file",
        "# only comments",
    ] {
        assert!(parse_patterns(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_pre_commit_hook_reads_the_same_pattern_file() {
    let hook = std::fs::read_to_string(root().join("scripts/hooks/pre-commit")).unwrap_or_default();
    if hook.is_empty() {
        eprintln!("secret_scan: skipped, scripts/hooks/pre-commit is not here");
        return;
    }
    assert!(
        hook.contains(PATTERNS),
        "scripts/hooks/pre-commit must scan staged files with {PATTERNS}"
    );
}
