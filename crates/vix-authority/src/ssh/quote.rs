//! Shell quoting for the SSH authority (T712).
//!
//! Two layers:
//!
//! - [`sh_quote`] single-quotes one word for a POSIX `sh`. Every remote path,
//!   argument and environment value goes through it, as bytes, so a name with
//!   spaces, quotes, `$`, newlines or invalid UTF-8 reaches the remote command
//!   unchanged.
//! - [`envelope`] wraps a whole `sh` script so it survives the user's *login*
//!   shell, which `sshd` runs first and which may be `fish` or `csh` rather
//!   than a POSIX shell. The script is sent as a word of octal digits that a
//!   fixed decoder (plain text with no backslash escape, `!` or newline in it)
//!   turns back into the script inside `/bin/sh`.

use std::path::Path;

/// Single-quote `word` for a POSIX shell: `'…'`, with each `'` written as
/// `'\''`. The result is one shell word that expands to exactly `word`
/// (which must not contain a NUL byte; no shell can carry one).
#[must_use]
pub fn sh_quote(word: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(word.len() + 2);
    out.push(b'\'');
    for &b in word {
        if b == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(b);
        }
    }
    out.push(b'\'');
    out
}

/// [`sh_quote`] for text.
#[must_use]
pub fn sh_quote_str(word: &str) -> String {
    String::from_utf8_lossy(&sh_quote(word.as_bytes())).into_owned()
}

/// An argument's bytes: exact on Unix, UTF-8 (lossy) elsewhere.
#[must_use]
pub fn os_bytes(text: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        text.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        text.to_string_lossy().into_owned().into_bytes()
    }
}

/// A remote path's bytes: exact on Unix; elsewhere UTF-8 (lossy) with `\`
/// turned into `/`, since the remote side is always a POSIX system and a
/// Windows `Path::join` uses `\`.
#[must_use]
pub fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        os_bytes(path.as_os_str())
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().replace('\\', "/").into_bytes()
    }
}

/// The path for `bytes` (the inverse of [`path_bytes`]).
#[must_use]
pub fn bytes_path(bytes: &[u8]) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        std::path::PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Quote a remote path. `~` and a leading `~/` mean the remote home
/// directory and become `"$HOME"` followed by the quoted rest; everything
/// else is [`sh_quote`]d as is.
#[must_use]
pub fn quote_path(path: &Path) -> Vec<u8> {
    let bytes = path_bytes(path);
    if bytes == b"~" {
        return b"\"$HOME\"".to_vec();
    }
    if let Some(rest) = bytes.strip_prefix(b"~/") {
        let mut out = b"\"$HOME\"/".to_vec();
        if !rest.is_empty() {
            out.extend(sh_quote(rest));
        }
        return out;
    }
    sh_quote(&bytes)
}

/// Escape `*`, `?`, `[` and `\` so `find -name` matches `name` literally.
#[must_use]
pub fn glob_escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '*' | '?' | '[' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The fixed decoder [`envelope`] runs in `/bin/sh`: it turns its first
/// argument (three octal digits per byte) back into the script and `eval`s
/// it. `tr X "\134"` makes the backslashes, so the decoder's own text holds
/// no `\\`, `\'`, `!` or newline, which `fish` and `csh` would treat
/// specially even inside single quotes.
pub const DECODER: &str =
    r#"eval "$(printf %b "$(printf %s "$1" | sed "s/.../X0&/g" | tr X "\134")")""#;

/// The remote command line that runs `script` in `/bin/sh` whatever the
/// user's login shell is: `exec /bin/sh -c '<DECODER>' vix <octal digits>`.
/// Only the digits vary, so nothing in `script` can be misread by the login
/// shell. The script's stdin is the SSH channel's stdin.
#[must_use]
pub fn envelope(script: &[u8]) -> String {
    format!("exec /bin/sh -c '{DECODER}' vix {}", octal(script))
}

/// The argument vector that runs `script` in `/bin/sh` when nothing parses
/// it as a command line first (`docker exec`, `kubectl exec`, `wsl.exe
/// --exec`; T718): `/bin/sh -c <DECODER> vix <octal digits>`. The same
/// decoder as [`envelope`], so the script is never re-quoted on the way:
/// whatever a platform's argv quoting does, it only ever sees the fixed
/// decoder and digits.
#[must_use]
pub fn sh_argv(script: &[u8]) -> [String; 5] {
    [
        "/bin/sh".into(),
        "-c".into(),
        DECODER.into(),
        "vix".into(),
        octal(script),
    ]
}

/// Three octal digits per byte.
fn octal(script: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut digits = String::with_capacity(script.len() * 3);
    for b in script {
        let _ = write!(digits, "{b:03o}");
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_wraps_and_escapes_single_quotes() {
        assert_eq!(sh_quote(b"a b"), b"'a b'");
        assert_eq!(sh_quote(b"it's"), b"'it'\\''s'");
        assert_eq!(sh_quote(b""), b"''");
        assert_eq!(sh_quote_str("$HOME"), "'$HOME'");
    }

    #[test]
    fn home_paths_expand_on_the_remote_side() {
        assert_eq!(quote_path(Path::new("~")), b"\"$HOME\"");
        assert_eq!(quote_path(Path::new("~/a b")), b"\"$HOME\"/'a b'");
        assert_eq!(quote_path(Path::new("/x/~")), b"'/x/~'");
        assert_eq!(quote_path(Path::new("~user")), b"'~user'");
    }

    #[test]
    fn find_patterns_are_literal() {
        assert_eq!(glob_escape("a*b?[c]\\"), "a\\*b\\?\\[c]\\\\");
    }

    #[test]
    fn the_decoder_is_safe_in_fish_and_csh_single_quotes() {
        for bad in ["\\\\", "\\'", "!", "\n", "'"] {
            assert!(!DECODER.contains(bad), "{bad:?}");
        }
        let e = envelope(b"echo 'hi'\n");
        assert!(e.ends_with(" vix 145143150157040047150151047012"), "{e}");
    }
}
