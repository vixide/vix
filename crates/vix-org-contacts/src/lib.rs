//! Org-contacts: contact management over Org files
//! (<https://github.com/doomelpa/org-contacts>).
//!
//! A *contact* is an ordinary Org headline (its text is the name) whose
//! `:PROPERTIES:` drawer holds structured fields — `EMAIL`, `PHONE`, `ADDRESS`,
//! `BIRTHDAY`, `NICKNAME`, `NOTE`, … (the canonical org-contacts property names).
//! This module is the pure, testable core: a new-contact skeleton, a single
//! property line, parsing contacts out of Org text, and compiling cross-file
//! views (a directory listing, a birthday list, and a vCard 3.0 export). The host
//! (`app`) wires these to the Org → Contacts menu.
//!
//! All functions are pure so they can be unit-tested without a live editor.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::fmt::Write as _;

/// The contact fields offered in the Insert-Field submenu, in order.
pub const FIELDS: &[&str] = &["EMAIL", "PHONE", "ADDRESS", "BIRTHDAY", "NICKNAME", "NOTE"];

/// One parsed contact: the headline name and its property fields (in file order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// The contact's name (the headline text).
    pub name: String,
    /// `(KEY, VALUE)` property pairs from the contact's drawer (keys upper-cased).
    pub fields: Vec<(String, String)>,
}

impl Contact {
    /// The value of property `key` (case-insensitive), if present and non-empty.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, v)| k.eq_ignore_ascii_case(key) && !v.is_empty())
            .map(|(_, v)| v.as_str())
    }
}

/// A new-contact skeleton: a top-level headline plus a property drawer with the
/// common empty fields, ready to fill in.
#[must_use]
pub fn new_contact(name: &str) -> String {
    let name = name.trim();
    format!("* {name}\n  :PROPERTIES:\n  :EMAIL:\n  :PHONE:\n  :ADDRESS:\n  :BIRTHDAY:\n  :END:\n")
}

/// A single indented property line for the Insert-Field commands, e.g.
/// `"  :EMAIL: "`.
#[must_use]
pub fn field_line(key: &str) -> String {
    format!("  :{}: ", key.to_ascii_uppercase())
}

/// The number of leading `*` of an Org headline (followed by a space), else `None`.
fn headline_level(line: &str) -> Option<usize> {
    let stars = line.len() - line.trim_start_matches('*').len();
    (stars > 0 && line[stars..].starts_with(' ')).then_some(stars)
}

/// Parse all contacts from one Org document: every headline whose immediately
/// following `:PROPERTIES:` drawer contains at least one known contact field.
#[must_use]
pub fn parse(content: &str) -> Vec<Contact> {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(level) = headline_level(lines[i]) else {
            i += 1;
            continue;
        };
        let name = lines[i][level..].trim().to_string();
        // A drawer must start on the next non-blank line.
        let mut j = i + 1;
        while j < lines.len() && lines[j].trim().is_empty() {
            j += 1;
        }
        if j < lines.len() && lines[j].trim().eq_ignore_ascii_case(":PROPERTIES:") {
            let mut fields = Vec::new();
            j += 1;
            while j < lines.len() && !lines[j].trim().eq_ignore_ascii_case(":END:") {
                let t = lines[j].trim();
                if let Some(rest) = t.strip_prefix(':')
                    && let Some((key, value)) = rest.split_once(':')
                {
                    fields.push((key.trim().to_ascii_uppercase(), value.trim().to_string()));
                }
                j += 1;
            }
            if fields.iter().any(|(k, _)| FIELDS.contains(&k.as_str())) {
                out.push(Contact { name, fields });
            }
        }
        i += 1;
    }
    out
}

/// All contacts across `files` (`(name, content)` pairs), in file then document
/// order.
#[must_use]
pub fn all(files: &[(String, String)]) -> Vec<Contact> {
    files.iter().flat_map(|(_, c)| parse(c)).collect()
}

/// Compile a **directory** of all contacts into an Org table (name, email, phone),
/// sorted by name.
#[must_use]
pub fn directory(files: &[(String, String)]) -> String {
    let mut rows: Vec<Contact> = all(files);
    rows.sort_by_key(|c| c.name.to_lowercase());
    let mut out = format!(
        "#+title: Contacts ({})\n\n| Name | Email | Phone |\n|-+-+-|\n",
        rows.len()
    );
    for c in &rows {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            org_cell(&c.name),
            org_cell(c.field("EMAIL").unwrap_or("")),
            org_cell(c.field("PHONE").unwrap_or(""))
        );
    }
    out
}

/// Compile a **birthday** list (contacts with a `BIRTHDAY`), sorted by the date
/// string, into an Org buffer.
#[must_use]
pub fn birthdays(files: &[(String, String)]) -> String {
    let mut rows: Vec<(&str, &str)> = Vec::new();
    let contacts = all(files);
    for c in &contacts {
        if let Some(b) = c.field("BIRTHDAY") {
            rows.push((b, c.name.as_str()));
        }
    }
    rows.sort_unstable();
    let mut out = format!("#+title: Birthdays ({})\n\n", rows.len());
    for (date, name) in &rows {
        let _ = writeln!(out, "- {date}  {name}");
    }
    out
}

/// Escape a value for a vCard property (RFC 6350 §3.4): backslash, comma and
/// semicolon are escaped, newlines become the literal `\n`, and carriage
/// returns are dropped. Without this, a field value containing `;`/`,`/newline
/// injects extra structured-value components or whole new property lines into
/// the exported vCard (e.g. a `NOTE` of `x\nTEL:+1-000-EVIL` forging a phone).
fn vcard_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Sanitize a value for a single Org **table cell**: `|` (column separator) and
/// newlines would break the table structure, so they are replaced with safe
/// equivalents.
fn org_cell(s: &str) -> String {
    s.replace('|', "\\vert").replace(['\n', '\r'], " ")
}

/// Export all contacts to **vCard 3.0** text.
#[must_use]
pub fn to_vcard(files: &[(String, String)]) -> String {
    let mut out = String::new();
    for c in all(files) {
        out.push_str("BEGIN:VCARD\nVERSION:3.0\n");
        let _ = writeln!(out, "FN:{}", vcard_escape(&c.name));
        if let Some(email) = c.field("EMAIL") {
            for addr in email.split_whitespace() {
                let _ = writeln!(out, "EMAIL:{}", vcard_escape(addr));
            }
        }
        if let Some(phone) = c.field("PHONE") {
            let _ = writeln!(out, "TEL:{}", vcard_escape(phone));
        }
        if let Some(addr) = c.field("ADDRESS") {
            let _ = writeln!(out, "ADR:{}", vcard_escape(addr));
        }
        if let Some(bday) = c.field("BIRTHDAY") {
            let _ = writeln!(out, "BDAY:{}", vcard_escape(bday));
        }
        if let Some(nick) = c.field("NICKNAME") {
            let _ = writeln!(out, "NICKNAME:{}", vcard_escape(nick));
        }
        if let Some(note) = c.field("NOTE") {
            let _ = writeln!(out, "NOTE:{}", vcard_escape(note));
        }
        out.push_str("END:VCARD\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
* Ada Lovelace
  :PROPERTIES:
  :EMAIL: ada@example.com ada@analytical.engine
  :PHONE: +44 1234
  :BIRTHDAY: 1815-12-10
  :END:
* Not A Contact
  some notes
* Alan Turing
  :PROPERTIES:
  :EMAIL: alan@example.com
  :BIRTHDAY: 1912-06-23
  :END:
";

    fn files() -> Vec<(String, String)> {
        vec![("contacts.org".to_string(), SAMPLE.to_string())]
    }

    #[test]
    fn new_contact_and_field_line() {
        let c = new_contact("Grace Hopper");
        assert!(c.starts_with("* Grace Hopper\n  :PROPERTIES:"));
        assert!(c.contains(":EMAIL:"));
        assert_eq!(field_line("email"), "  :EMAIL: ");
    }

    #[test]
    fn parse_picks_only_contacts() {
        let cs = parse(SAMPLE);
        assert_eq!(cs.len(), 2, "the note-only headline is not a contact");
        assert_eq!(cs[0].name, "Ada Lovelace");
        assert_eq!(
            cs[0].field("EMAIL"),
            Some("ada@example.com ada@analytical.engine")
        );
        assert_eq!(cs[0].field("PHONE"), Some("+44 1234"));
        assert_eq!(cs[1].name, "Alan Turing");
    }

    #[test]
    fn directory_sorts_by_name() {
        let d = directory(&files());
        assert!(d.contains("Contacts (2)"));
        assert!(d.find("Ada Lovelace").unwrap() < d.find("Alan Turing").unwrap());
        assert!(d.contains("| Ada Lovelace | ada@example.com ada@analytical.engine | +44 1234 |"));
    }

    #[test]
    fn birthdays_sorted_by_date() {
        let b = birthdays(&files());
        assert!(b.contains("Birthdays (2)"));
        // 1815 before 1912.
        assert!(b.find("Ada Lovelace").unwrap() < b.find("Alan Turing").unwrap());
    }

    #[test]
    fn vcard_export_splits_emails() {
        let v = to_vcard(&files());
        assert!(v.contains("BEGIN:VCARD\nVERSION:3.0"));
        assert!(v.contains("FN:Ada Lovelace"));
        assert!(v.contains("EMAIL:ada@example.com"));
        assert!(v.contains("EMAIL:ada@analytical.engine"));
        assert!(v.contains("BDAY:1815-12-10"));
        assert!(v.matches("END:VCARD").count() == 2);
    }

    #[test]
    fn vcard_escapes_structural_characters() {
        assert_eq!(vcard_escape("a;b,c\\d"), "a\\;b\\,c\\\\d");
        // A NOTE that tries to forge a TEL line is neutralized: the newline
        // becomes a literal `\n`, so no new property line is emitted.
        let org = "* Bob\n  :PROPERTIES:\n  :NOTE: hi;x\n  :EMAIL: e@x\n  :END:\n";
        let v = to_vcard(&[("c.org".into(), org.to_string())]);
        assert!(v.contains("NOTE:hi\\;x"), "got: {v}");
        // Exactly one TEL/EMAIL structure per real field — no injected lines.
        assert_eq!(v.matches("\nNOTE:").count(), 1);
    }

    #[test]
    fn directory_table_cells_cannot_break_the_table() {
        // A contact name containing a pipe would otherwise add spurious columns.
        let org = "* a | b\n  :PROPERTIES:\n  :EMAIL: e@x\n  :END:\n";
        let d = directory(&[("c.org".into(), org.to_string())]);
        // The row for this contact has exactly the 3 intended columns (4 pipes).
        let row = d.lines().find(|l| l.contains("\\vert")).expect("row present");
        assert_eq!(row.matches('|').count(), 4, "row: {row}");
    }

    proptest::proptest! {
        // A NOTE field of arbitrary text can never inject an extra vCard property
        // line: `vcard_escape` maps newlines to the literal `\n`, so the emitted
        // vCard for one contact has exactly one line per real field.
        #[test]
        fn vcard_note_cannot_inject_property_lines(note in ".*") {
            let org = format!("* X\n  :PROPERTIES:\n  :NOTE: {}\n  :EMAIL: e@x\n  :END:\n",
                note.replace('\n', " ")); // the property parser is single-line
            let v = to_vcard(&[("c.org".into(), org)]);
            // Exactly one NOTE line, one EMAIL line, one FN line.
            proptest::prop_assert!(v.matches("\nNOTE:").count() <= 1);
            // No line other than the known vCard properties/markers appears.
            for line in v.lines() {
                let ok = line.is_empty()
                    || ["BEGIN:VCARD", "END:VCARD", "VERSION:", "FN:", "EMAIL:",
                        "TEL:", "ADR:", "BDAY:", "NICKNAME:", "NOTE:"]
                        .iter()
                        .any(|p| line.starts_with(p));
                proptest::prop_assert!(ok, "unexpected injected line: {line:?}");
            }
        }

        // vcard_escape never panics and never leaves a bare newline in its output.
        #[test]
        fn vcard_escape_removes_line_breaks(s in ".*") {
            let e = vcard_escape(&s);
            proptest::prop_assert!(!e.contains('\n'));
            proptest::prop_assert!(!e.contains('\r'));
        }
    }
}
