//! Local crash reports (`spec/crash-reports/index.md`).
//!
//! When Vix panics, [`install_hook`]'s hook writes a plain-text report to
//! `<data dir>/crashes/` before the terminal is restored. The report holds the
//! Vix version, the operating system and architecture, the panic message and
//! location, and a backtrace, with the home directory, the user name and any
//! quoted text taken out ([`scrub`]). It never holds buffer text, file names
//! from the workspace beyond what a backtrace frame names, or anything from
//! the environment.
//!
//! Nothing is sent anywhere. On the next start Vix says a report is waiting;
//! **Help → Report a Crash…** shows the newest one and, if asked, opens a
//! prefilled GitHub issue in the browser, where the user reviews it and
//! decides whether to submit.

use std::fs;
use std::path::{Path, PathBuf};

/// How many reports are kept; older ones are deleted when a new one is written.
pub const KEEP: usize = 10;

/// Longest panic message kept in a report.
const MAX_MESSAGE: usize = 500;

/// Longest prefilled issue body, before URL encoding, so the URL stays under
/// the length browsers and GitHub accept.
const MAX_ISSUE_BODY: usize = 4000;

/// The file that records the newest report the user has already been told
/// about.
const SEEN_FILE: &str = "last-seen";

/// Where crash reports go: `<data dir>/crashes/`.
#[must_use]
pub fn crashes_dir() -> Option<PathBuf> {
    crate::settings::Settings::data_dir().map(|d| d.join("crashes"))
}

/// Install the panic hook that writes a report, in front of the hook already
/// installed (ratatui's, which restores the terminal and prints the panic).
pub fn install_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let saved = crashes_dir().and_then(|dir| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let location = info
                .location()
                .map_or_default(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let report = report_text(&Crash {
                message: &message,
                location: &location,
                backtrace: &backtrace,
            });
            save(&dir, &report, &timestamp()).ok()
        });
        previous(info);
        if let Some(path) = saved {
            eprintln!(
                "vix: a crash report was saved to {} (nothing was sent). \
                 Help → Report a Crash… can open it as a GitHub issue.",
                path.display()
            );
        }
    }));
}

/// What a panic reported.
#[derive(Debug, Clone, Copy)]
pub struct Crash<'a> {
    /// The panic payload, as text.
    pub message: &'a str,
    /// `file:line:column` of the panic, in Vix's source.
    pub location: &'a str,
    /// The captured backtrace.
    pub backtrace: &'a str,
}

/// The report text for `crash`, scrubbed.
#[must_use]
pub fn report_text(crash: &Crash<'_>) -> String {
    let home = home_dir();
    let user = user_name();
    let scrub = |s: &str| scrub(s, home.as_deref(), user.as_deref());
    let message = scrub(&redact_quoted(crash.message));
    let message: String = message.chars().take(MAX_MESSAGE).collect();
    format!(
        "Vix crash report\n\
         version: {}\n\
         os: {} {}\n\
         panic: {}\n\
         at: {}\n\
         \n\
         backtrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        message.trim(),
        scrub(crash.location),
        scrub(crash.backtrace).trim_end()
    )
}

/// `text` with the home directory replaced by `~` and the user name (when it
/// is at least three characters, so it cannot match everywhere) by `<user>`.
#[must_use]
pub fn scrub(text: &str, home: Option<&str>, user: Option<&str>) -> String {
    let mut out = text.to_string();
    if let Some(home) = home.filter(|h| h.len() > 1) {
        out = out.replace(home, "~");
    }
    if let Some(user) = user.filter(|u| u.chars().count() >= 3) {
        out = out.replace(user, "<user>");
    }
    out
}

/// `message` with quoted text removed: anything between backquotes (the
/// standard library quotes string contents that way, as in "byte index 3 is
/// not a char boundary; … of `…`"), and double-quoted text longer than 40
/// characters (such as a failed `assert_eq!`'s values).
#[must_use]
pub fn redact_quoted(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(open) = rest.find(['`', '"']) {
        let quote = rest[open..].chars().next().unwrap_or('`');
        let Some(len) = rest[open + 1..].find(quote) else {
            break;
        };
        let inner = &rest[open + 1..open + 1 + len];
        out.push_str(&rest[..open]);
        if quote == '`' || inner.chars().count() > 40 {
            out.push(quote);
            out.push_str("<removed>");
            out.push(quote);
        } else {
            out.push_str(&rest[open..=open + 1 + len]);
        }
        rest = &rest[open + 2 + len..];
    }
    out.push_str(rest);
    out
}

/// Write `report` as `crash-<stamp>.txt` in `dir`, then delete all but the
/// newest [`KEEP`] reports. Returns the new report's path.
///
/// # Errors
///
/// Returns the I/O error if the directory or the file cannot be written.
pub fn save(dir: &Path, report: &str, stamp: &str) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("crash-{stamp}.txt"));
    fs::write(&path, report)?;
    let reports = reports(dir);
    for old in reports.iter().skip(KEEP) {
        let _ = fs::remove_file(old);
    }
    Ok(path)
}

/// The reports in `dir`, newest first.
#[must_use]
pub fn reports(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "txt")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("crash-"))
        })
        .collect();
    // The names sort by time (`crash-<UTC stamp>.txt`).
    found.sort_unstable_by(|a, b| b.cmp(a));
    found
}

/// The newest report the user has not been told about yet, marking it told.
#[must_use]
pub fn take_unseen(dir: &Path) -> Option<PathBuf> {
    let newest = reports(dir).into_iter().next()?;
    let name = newest.file_name()?.to_string_lossy().into_owned();
    let seen = fs::read_to_string(dir.join(SEEN_FILE)).unwrap_or_default();
    if seen.trim() == name {
        return None;
    }
    let _ = fs::write(dir.join(SEEN_FILE), &name);
    Some(newest)
}

/// A prefilled "new issue" URL for `report` on `repository` (the GitHub
/// repository URL), using the bug report form's fields.
#[must_use]
pub fn issue_url(repository: &str, report: &str) -> String {
    let mut body: String = report.chars().take(MAX_ISSUE_BODY).collect();
    if body.len() < report.len() {
        body.push_str("\n[… truncated: the full report is in Vix's crashes folder]");
    }
    let body = format!("Vix crashed. The crash report it saved:\n\n```text\n{body}\n```");
    let field = |line: &str| {
        report
            .lines()
            .find_map(|l| l.strip_prefix(line))
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let title = format!(
        "Crash: {}",
        field("panic: ").chars().take(80).collect::<String>()
    );
    format!(
        "{}/issues/new?template=bug_report.yml&title={}&what-happened={}&version={}&os={}",
        repository.trim_end_matches('/'),
        encode(&title),
        encode(&body),
        encode(&field("version: ")),
        encode(&field("os: "))
    )
}

/// Percent-encode `text` for a URL query value.
fn encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len() * 3);
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        }
    }
    out
}

/// The current UTC time as `YYYYMMDDTHHMMSSZ`, sortable, for a report name.
fn timestamp() -> String {
    jiff::Timestamp::now()
        .strftime("%Y%m%dT%H%M%SZ")
        .to_string()
}

fn home_dir() -> Option<String> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|h| h.to_string_lossy().into_owned())
}

fn user_name() -> Option<String> {
    std::env::var_os(if cfg!(windows) { "USERNAME" } else { "USER" })
        .map(|u| u.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vix-crash-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn scrub_hides_home_and_user() {
        let s = scrub(
            "/home/ana/src/x.rs and /home/ana/.cargo ana",
            Some("/home/ana"),
            Some("ana"),
        );
        assert_eq!(s, "~/src/x.rs and ~/.cargo <user>");
        // A short user name would match everywhere, so it is left alone.
        assert_eq!(scrub("a b", None, Some("a")), "a b");
        assert_eq!(scrub("/x", Some("/"), None), "/x");
    }

    #[test]
    fn redact_removes_backquoted_and_long_quoted_text() {
        assert_eq!(
            redact_quoted("byte index 3 is not a char boundary; it is inside 'é' of `secret text`"),
            "byte index 3 is not a char boundary; it is inside 'é' of `<removed>`"
        );
        let long = format!("left: \"{}\", right: \"ok\"", "x".repeat(50));
        assert_eq!(redact_quoted(&long), "left: \"<removed>\", right: \"ok\"");
        assert_eq!(redact_quoted("unclosed `quote"), "unclosed `quote");
        assert_eq!(redact_quoted("plain"), "plain");
    }

    #[test]
    fn report_text_has_the_fields_and_no_secrets() {
        let r = report_text(&Crash {
            message: "boom of `my private line`",
            location: "tui/src/app.rs:10:5",
            backtrace: "   0: vix::app::run\n",
        });
        assert!(r.starts_with("Vix crash report\nversion: "), "{r}");
        assert!(r.contains(&format!(
            "os: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )));
        assert!(r.contains("panic: boom of `<removed>`"), "{r}");
        assert!(r.contains("at: tui/src/app.rs:10:5"));
        assert!(r.contains("backtrace:\n   0: vix::app::run"));
        assert!(!r.contains("my private line"));
        let long = "y".repeat(MAX_MESSAGE * 2);
        let r = report_text(&Crash {
            message: &long,
            location: "",
            backtrace: "",
        });
        assert!(r.contains(&"y".repeat(MAX_MESSAGE)) && !r.contains(&"y".repeat(MAX_MESSAGE + 1)));
    }

    #[test]
    fn save_keeps_the_newest_reports() {
        let dir = scratch("keep");
        for i in 0..(KEEP + 3) {
            save(&dir, "r", &format!("20260101T0000{i:02}Z")).unwrap();
        }
        let kept = reports(&dir);
        assert_eq!(kept.len(), KEEP);
        assert!(kept[0].ends_with(format!("crash-20260101T0000{:02}Z.txt", KEEP + 2)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn take_unseen_tells_once_per_new_report() {
        let dir = scratch("seen");
        assert_eq!(take_unseen(&dir), None);
        let first = save(&dir, "r", "20260101T000000Z").unwrap();
        assert_eq!(take_unseen(&dir), Some(first));
        assert_eq!(take_unseen(&dir), None);
        let second = save(&dir, "r", "20260102T000000Z").unwrap();
        assert_eq!(take_unseen(&dir), Some(second));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn issue_url_prefills_the_bug_form() {
        let report = "Vix crash report\nversion: 2.0.2\nos: macos aarch64\npanic: index out of bounds\nat: x\n";
        let url = issue_url("https://github.com/vixide/vix/", report);
        assert!(url.starts_with(
            "https://github.com/vixide/vix/issues/new?template=bug_report.yml&title=Crash%3A%20index%20out%20of%20bounds&what-happened="
        ), "{url}");
        assert!(url.ends_with("&version=2.0.2&os=macos%20aarch64"), "{url}");
        let huge = format!("{report}{}", "z".repeat(MAX_ISSUE_BODY * 2));
        let url = issue_url("https://github.com/vixide/vix", &huge);
        assert!(url.contains("truncated"));
        assert!(url.len() < MAX_ISSUE_BODY * 3 + 1000);
    }

    #[test]
    fn encode_escapes_everything_but_unreserved() {
        assert_eq!(encode("a b/é&"), "a%20b%2F%C3%A9%26");
    }
}
