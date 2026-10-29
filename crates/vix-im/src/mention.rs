//! Nick comparison under IRC case mapping, and `@nick` mention detection.
//!
//! IRC servers declare how nicks compare in `ISUPPORT CASEMAPPING`; two nicks
//! that fold to the same string are the same person. The fold applies to
//! ASCII only (non-ASCII characters compare exactly), as in every IRC server.

use std::ops::Range;

/// How nicks (and channel names) fold before they are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum CaseMapping {
    /// `A`–`Z` fold to `a`–`z` and `[]\~` to `{}|^` (RFC 1459; IRC's default).
    #[default]
    Rfc1459,
    /// `A`–`Z` fold to `a`–`z` only.
    Ascii,
    /// `A`–`Z` fold to `a`–`z` and `[]\` to `{}|`, but `~` stays itself.
    StrictRfc1459,
}

impl CaseMapping {
    /// Read the `CASEMAPPING` value a server declares (`rfc1459`, `ascii`,
    /// `strict-rfc1459`, any case). `None` for a mapping Vix does not know;
    /// callers then keep [`CaseMapping::default`].
    #[must_use]
    pub fn from_isupport(value: &str) -> Option<CaseMapping> {
        match value.to_ascii_lowercase().as_str() {
            "rfc1459" => Some(CaseMapping::Rfc1459),
            "ascii" => Some(CaseMapping::Ascii),
            "strict-rfc1459" => Some(CaseMapping::StrictRfc1459),
            _ => None,
        }
    }

    /// The `CASEMAPPING` token for this mapping.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CaseMapping::Rfc1459 => "rfc1459",
            CaseMapping::Ascii => "ascii",
            CaseMapping::StrictRfc1459 => "strict-rfc1459",
        }
    }

    /// Fold one character.
    #[must_use]
    pub fn fold_char(self, c: char) -> char {
        match (self, c) {
            (_, 'A'..='Z') => c.to_ascii_lowercase(),
            (CaseMapping::Rfc1459 | CaseMapping::StrictRfc1459, '[') => '{',
            (CaseMapping::Rfc1459 | CaseMapping::StrictRfc1459, ']') => '}',
            (CaseMapping::Rfc1459 | CaseMapping::StrictRfc1459, '\\') => '|',
            (CaseMapping::Rfc1459, '~') => '^',
            _ => c,
        }
    }

    /// Fold a whole nick.
    #[must_use]
    pub fn fold(self, nick: &str) -> String {
        nick.chars().map(|c| self.fold_char(c)).collect()
    }

    /// Whether two nicks name the same person.
    #[must_use]
    pub fn nick_eq(self, a: &str, b: &str) -> bool {
        a.chars().count() == b.chars().count()
            && a.chars()
                .zip(b.chars())
                .all(|(x, y)| self.fold_char(x) == self.fold_char(y))
    }
}

/// Whether `c` can be part of a nick: an ASCII letter or digit, one of
/// IRC's special nick characters, or any other non-ASCII character that is
/// not white space or common punctuation. Non-ASCII is taken broadly so that
/// combining marks (Devanagari vowel signs, Arabic harakat) and the
/// zero-width joiners stay inside a nick, without a Unicode-tables
/// dependency.
#[must_use]
pub fn is_nick_char(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric()
            || matches!(
                c,
                '-' | '_' | '[' | ']' | '\\' | '`' | '^' | '{' | '}' | '|'
            );
    }
    !(c.is_whitespace() || c.is_control() || is_wide_punctuation(c))
}

/// Common non-ASCII punctuation that ends a nick.
fn is_wide_punctuation(c: char) -> bool {
    matches!(c,
        // General Punctuation, less the zero-width (non-)joiner.
        '\u{2000}'..='\u{200B}' | '\u{200E}'..='\u{206F}'
        // CJK symbols and punctuation; full-width ASCII punctuation.
        | '\u{3000}'..='\u{303F}'
        | '\u{FF01}'..='\u{FF0F}' | '\u{FF1A}'..='\u{FF20}'
        | '\u{FF3B}'..='\u{FF40}' | '\u{FF5B}'..='\u{FF65}'
        // Latin-1 punctuation and symbols.
        | '\u{A0}'..='\u{BF}' | '\u{D7}' | '\u{F7}'
        // Arabic comma, semicolon, question mark, full stop; Devanagari danda.
        | '\u{060C}' | '\u{061B}' | '\u{061F}' | '\u{06D4}' | '\u{0964}' | '\u{0965}'
    )
}

/// Every `@nick` in `body`: the byte range of the whole mention (including
/// the `@`) and the nick. A mention starts at the beginning of the text or
/// after a character that cannot be in a nick (so `ana@example.org` is not a
/// mention), and the nick runs as far as nick characters go.
#[must_use]
pub fn find_mentions(body: &str) -> Vec<(Range<usize>, &str)> {
    let mut found = Vec::new();
    let mut prev: Option<char> = None;
    for (at, c) in body.char_indices() {
        let starts = c == '@' && !prev.is_some_and(|p| is_nick_char(p) || p == '@');
        prev = Some(c);
        if !starts {
            continue;
        }
        let rest = &body[at + 1..];
        let len = rest
            .char_indices()
            .find(|&(_, c)| !is_nick_char(c))
            .map_or(rest.len(), |(i, _)| i);
        if len > 0 {
            found.push((at..at + 1 + len, &rest[..len]));
        }
    }
    found
}

/// Whether `body` mentions `nick` (as `@nick`) under `mapping`. An empty
/// nick is never mentioned.
#[must_use]
pub fn mentions(body: &str, nick: &str, mapping: CaseMapping) -> bool {
    !nick.is_empty()
        && find_mentions(body)
            .iter()
            .any(|(_, n)| mapping.nick_eq(n, nick))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_from_isupport() {
        assert_eq!(
            CaseMapping::from_isupport("RFC1459"),
            Some(CaseMapping::Rfc1459)
        );
        assert_eq!(
            CaseMapping::from_isupport("ascii"),
            Some(CaseMapping::Ascii)
        );
        assert_eq!(
            CaseMapping::from_isupport("strict-rfc1459"),
            Some(CaseMapping::StrictRfc1459)
        );
        assert_eq!(CaseMapping::from_isupport("rfc7613"), None);
        for m in [
            CaseMapping::Rfc1459,
            CaseMapping::Ascii,
            CaseMapping::StrictRfc1459,
        ] {
            assert_eq!(CaseMapping::from_isupport(m.as_str()), Some(m));
        }
    }

    #[test]
    fn folding_differs_by_mapping() {
        assert!(CaseMapping::Rfc1459.nick_eq("Ana[m]~", "ana{m}^"));
        assert!(!CaseMapping::StrictRfc1459.nick_eq("Ana[m]~", "ana{m}^"));
        assert!(CaseMapping::StrictRfc1459.nick_eq("Ana[m]\\", "ana{m}|"));
        assert!(!CaseMapping::Ascii.nick_eq("ana[", "ana{"));
        assert!(CaseMapping::Ascii.nick_eq("ANA", "ana"));
        // Non-ASCII compares exactly.
        assert!(!CaseMapping::Rfc1459.nick_eq("Ünal", "ünal"));
        assert!(!CaseMapping::Ascii.nick_eq("ana", "anaa"));
    }

    #[test]
    fn finds_mentions() {
        let body = "@ana, ask @Bo[t] — not mail@ana.org or @@x or @";
        let nicks: Vec<&str> = find_mentions(body).into_iter().map(|(_, n)| n).collect();
        assert_eq!(nicks, ["ana", "Bo[t]"]);
        let (range, _) = &find_mentions(body)[0];
        assert_eq!(&body[range.clone()], "@ana");
    }

    #[test]
    fn mention_any_script() {
        assert!(mentions("@علی، سلام", "علی", CaseMapping::Rfc1459));
        assert!(mentions("「@ana」", "ana", CaseMapping::Rfc1459));
        assert!(mentions("@ana—later", "ana", CaseMapping::Rfc1459));
        assert!(mentions(
            "@می\u{200C}روم hi",
            "می\u{200C}روم",
            CaseMapping::Rfc1459
        ));
        assert!(mentions("سلام @علی", "علی", CaseMapping::Rfc1459));
        assert!(mentions("ping @नमस्ते!", "नमस्ते", CaseMapping::Ascii));
    }

    #[test]
    fn mention_uses_mapping() {
        assert!(mentions("hi @ANA[1]", "ana{1}", CaseMapping::Rfc1459));
        assert!(!mentions("hi @ANA[1]", "ana{1}", CaseMapping::Ascii));
        assert!(!mentions("hi @anab", "ana", CaseMapping::Rfc1459));
        assert!(!mentions("hi @ana", "", CaseMapping::Rfc1459));
    }
}
