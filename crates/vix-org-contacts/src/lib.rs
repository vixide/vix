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
            c.name,
            c.field("EMAIL").unwrap_or(""),
            c.field("PHONE").unwrap_or("")
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

/// Export all contacts to **vCard 3.0** text.
#[must_use]
pub fn to_vcard(files: &[(String, String)]) -> String {
    let mut out = String::new();
    for c in all(files) {
        out.push_str("BEGIN:VCARD\nVERSION:3.0\n");
        let _ = writeln!(out, "FN:{}", c.name);
        if let Some(email) = c.field("EMAIL") {
            for addr in email.split_whitespace() {
                let _ = writeln!(out, "EMAIL:{addr}");
            }
        }
        if let Some(phone) = c.field("PHONE") {
            let _ = writeln!(out, "TEL:{phone}");
        }
        if let Some(addr) = c.field("ADDRESS") {
            let _ = writeln!(out, "ADR:{addr}");
        }
        if let Some(bday) = c.field("BIRTHDAY") {
            let _ = writeln!(out, "BDAY:{bday}");
        }
        if let Some(nick) = c.field("NICKNAME") {
            let _ = writeln!(out, "NICKNAME:{nick}");
        }
        if let Some(note) = c.field("NOTE") {
            let _ = writeln!(out, "NOTE:{note}");
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
}
