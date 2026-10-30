//! `stat(1)` differs between GNU coreutils (and `BusyBox`), which take
//! `-c FORMAT`, and the BSDs and macOS, which take `-f FORMAT` with other
//! format letters. The SSH authority probes once per connection and then
//! formats and parses for the flavor it found (T712).

use std::time::{Duration, SystemTime};

use crate::{FileKind, FileStat};

/// Which `stat(1)` the remote machine has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatFlavor {
    /// GNU coreutils or `BusyBox`: `stat -c '%f %s %Y %W'` (mode in hex).
    Gnu,
    /// BSD or macOS: `stat -f '%p %z %m %B'` (mode in octal).
    Bsd,
}

/// The script that prints `gnu` or `bsd`.
pub(crate) const PROBE: &str = "if stat -c %s / >/dev/null 2>&1; then echo gnu; else echo bsd; fi";

impl StatFlavor {
    /// The flavor a [`PROBE`] reply names.
    pub(crate) fn from_probe(reply: &[u8]) -> Option<Self> {
        match String::from_utf8_lossy(reply).trim() {
            "gnu" => Some(Self::Gnu),
            "bsd" => Some(Self::Bsd),
            _ => None,
        }
    }

    /// The `stat` command (without the path) printing `mode size mtime
    /// birth` on one line; `follow` follows a symlink.
    pub(crate) fn command(self, follow: bool) -> &'static str {
        match (self, follow) {
            (Self::Gnu, true) => "stat -L -c '%f %s %Y %W' --",
            (Self::Gnu, false) => "stat -c '%f %s %Y %W' --",
            (Self::Bsd, true) => "stat -L -f '%p %z %m %B' --",
            (Self::Bsd, false) => "stat -f '%p %z %m %B' --",
        }
    }

    /// The `stat` command (without the path) printing just the permission
    /// bits in octal, the form `chmod` takes.
    pub(crate) fn mode_command(self) -> &'static str {
        match self {
            Self::Gnu => "stat -L -c %a --",
            Self::Bsd => "stat -L -f %Lp --",
        }
    }

    /// Parse one `mode size mtime birth` line.
    pub(crate) fn parse(self, line: &[u8]) -> Option<FileStat> {
        let text = String::from_utf8_lossy(line);
        let mut fields = text.split_whitespace();
        let radix = match self {
            Self::Gnu => 16,
            Self::Bsd => 8,
        };
        let mode = u32::from_str_radix(fields.next()?, radix).ok()?;
        let len = fields.next()?.parse::<u64>().ok()?;
        let modified = fields.next().and_then(epoch);
        let created = fields.next().and_then(epoch);
        let kind = match mode & 0o170_000 {
            0o100_000 => FileKind::File,
            0o040_000 => FileKind::Dir,
            0o120_000 => FileKind::Symlink,
            _ => FileKind::Other,
        };
        Some(
            FileStat::new(kind, len)
                .with_modified(modified)
                .with_created(created)
                .with_readonly(mode & 0o222 == 0)
                .with_mode(Some(mode)),
        )
    }
}

/// Seconds since the epoch, as a time; `0`, `-` or `?` (unknown) are `None`.
fn epoch(field: &str) -> Option<SystemTime> {
    let secs = field.parse::<i64>().ok().filter(|&s| s != 0)?;
    let magnitude = Duration::from_secs(secs.unsigned_abs());
    if secs > 0 {
        SystemTime::UNIX_EPOCH.checked_add(magnitude)
    } else {
        SystemTime::UNIX_EPOCH.checked_sub(magnitude)
    }
}

/// Whole seconds since the epoch, the precision the remote `stat` reports.
pub(crate) fn epoch_secs(t: SystemTime) -> Option<i64> {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok(),
        Err(e) => i64::try_from(e.duration().as_secs()).ok().map(|s| -s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnu_and_bsd_replies_parse_alike() {
        let gnu = StatFlavor::Gnu.parse(b"81a4 6 1793320383 0\n").unwrap();
        let bsd = StatFlavor::Bsd
            .parse(b"100644 6 1793320383 1793320000\n")
            .unwrap();
        for st in [gnu, bsd] {
            assert!(st.is_file());
            assert_eq!(st.len, 6);
            assert_eq!(st.modified.and_then(epoch_secs), Some(1_793_320_383));
            assert!(!st.readonly);
            assert_eq!(st.mode, Some(0o100_644));
        }
        assert_eq!(gnu.created, None, "GNU reports 0 for an unknown birth time");
        assert!(bsd.created.is_some());
        let dir = StatFlavor::Gnu.parse(b"41ed 4096 1 -").unwrap();
        assert!(dir.is_dir() && dir.created.is_none());
        let link = StatFlavor::Bsd.parse(b"120755 3 1 1").unwrap();
        assert!(link.is_symlink());
        let ro = StatFlavor::Bsd.parse(b"100444 0 1 1").unwrap();
        assert!(ro.readonly);
        let fifo = StatFlavor::Gnu.parse(b"11a4 0 1 0").unwrap();
        assert_eq!(fifo.kind, FileKind::Other);
        assert_eq!(StatFlavor::Gnu.parse(b"garbage"), None);
        assert_eq!(StatFlavor::from_probe(b"bsd\n"), Some(StatFlavor::Bsd));
        assert_eq!(StatFlavor::from_probe(b"?"), None);
    }
}
