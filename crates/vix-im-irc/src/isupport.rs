//! `RPL_ISUPPORT` (`005`) tokens this client acts on: `CASEMAPPING`,
//! `NICKLEN` and `CHANTYPES`.

/// How the server folds case when it compares nicks and channel names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum CaseMapping {
    /// `ascii`: only `A-Z` fold to `a-z`.
    Ascii,
    /// `rfc1459` (the RFC default): ASCII plus `[]\~` fold to `{}|^`.
    #[default]
    Rfc1459,
    /// `strict-rfc1459`: ASCII plus `[]\` fold to `{}|` (not `~`).
    StrictRfc1459,
}

impl CaseMapping {
    /// The mapping a `CASEMAPPING=` value names. An unknown value (for
    /// example `rfc7613`, which needs Unicode normalization this crate does
    /// not do) falls back to `ascii`, the safe common subset.
    #[must_use]
    pub fn from_token(value: &str) -> CaseMapping {
        match value.to_ascii_lowercase().as_str() {
            "rfc1459" => CaseMapping::Rfc1459,
            "strict-rfc1459" => CaseMapping::StrictRfc1459,
            _ => CaseMapping::Ascii,
        }
    }

    /// Fold `text` to its canonical lower-case form under this mapping.
    #[must_use]
    pub fn fold(self, text: &str) -> String {
        text.chars()
            .map(|c| match (self, c) {
                (_, 'A'..='Z') => c.to_ascii_lowercase(),
                (CaseMapping::Rfc1459 | CaseMapping::StrictRfc1459, '[') => '{',
                (CaseMapping::Rfc1459 | CaseMapping::StrictRfc1459, ']') => '}',
                (CaseMapping::Rfc1459 | CaseMapping::StrictRfc1459, '\\') => '|',
                (CaseMapping::Rfc1459, '~') => '^',
                _ => c,
            })
            .collect()
    }

    /// Whether two names are the same under this mapping.
    #[must_use]
    pub fn same(self, a: &str, b: &str) -> bool {
        a.len() == b.len() && self.fold(a) == self.fold(b)
    }
}

/// The server's declared limits and conventions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isupport {
    /// `CASEMAPPING` (default `rfc1459`).
    pub casemapping: CaseMapping,
    /// `NICKLEN`, when the server declared one.
    pub nicklen: Option<usize>,
    /// `CHANTYPES`: the characters a channel name may start with
    /// (default `#&`).
    pub chantypes: String,
}

impl Default for Isupport {
    /// RFC 1459 casemapping, no nick-length limit, and the classic `#`/`&` channel prefixes: what a server is assumed to be before (or without) an `ISUPPORT` advertisement.
    fn default() -> Self {
        Isupport {
            casemapping: CaseMapping::default(),
            nicklen: None,
            chantypes: "#&".to_string(),
        }
    }
}

impl Isupport {
    /// Apply the tokens of one `005` line (its parameters after the client's
    /// nick; the final human-readable parameter is ignored since it holds
    /// spaces). `-TOKEN` resets a token to its default.
    pub fn apply<S: AsRef<str>>(&mut self, tokens: &[S]) {
        for token in tokens {
            let token = token.as_ref();
            if token.contains(' ') {
                continue;
            }
            if let Some(name) = token.strip_prefix('-') {
                let fresh = Isupport::default();
                match name {
                    "CASEMAPPING" => self.casemapping = fresh.casemapping,
                    "NICKLEN" => self.nicklen = fresh.nicklen,
                    "CHANTYPES" => self.chantypes = fresh.chantypes,
                    _ => {}
                }
                continue;
            }
            let (name, value) = token.split_once('=').unwrap_or((token, ""));
            match name {
                "CASEMAPPING" => self.casemapping = CaseMapping::from_token(value),
                "NICKLEN" => self.nicklen = value.parse().ok().filter(|n| *n > 0),
                "CHANTYPES" => {
                    value.clone_into(&mut self.chantypes);
                }
                _ => {}
            }
        }
    }

    /// Whether `target` names a channel (starts with one of `CHANTYPES`).
    #[must_use]
    pub fn is_channel(&self, target: &str) -> bool {
        target
            .chars()
            .next()
            .is_some_and(|c| self.chantypes.contains(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casemapping_folds() {
        assert!(CaseMapping::Rfc1459.same("Ana[x]~", "ana{x}^"));
        assert!(CaseMapping::StrictRfc1459.same("Ana[x]", "ana{x}"));
        assert!(!CaseMapping::StrictRfc1459.same("a~", "a^"));
        assert!(!CaseMapping::Ascii.same("a[", "a{"));
        assert!(CaseMapping::Ascii.same("ANA", "ana"));
        // Non-ASCII letters are not folded by any IRC mapping here.
        assert!(!CaseMapping::Ascii.same("Ä", "ä"));
    }

    #[test]
    fn applies_tokens() {
        let mut s = Isupport::default();
        assert!(s.is_channel("#vix"));
        assert!(!s.is_channel("ana"));
        s.apply(&[
            "CASEMAPPING=ascii",
            "NICKLEN=16",
            "CHANTYPES=#",
            "are supported by this server",
        ]);
        assert_eq!(s.casemapping, CaseMapping::Ascii);
        assert_eq!(s.nicklen, Some(16));
        assert!(!s.is_channel("&local"));
        s.apply(&["-CHANTYPES", "NICKLEN=x", "CASEMAPPING=rfc7613"]);
        assert_eq!(s.chantypes, "#&");
        assert_eq!(s.nicklen, None);
        assert_eq!(s.casemapping, CaseMapping::Ascii);
        s.apply(&["CHANTYPES="]);
        assert!(!s.is_channel("#vix"));
    }
}
