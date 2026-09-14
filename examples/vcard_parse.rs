//! Parse Org-contacts headlines and export them as vCard 3.0 (improvement
//! plan T503, `tasks.md`, whose "`vcard_parse`" name describes the *result*
//! more than the direction: `vix-org-contacts` parses Org text into
//! [`Contact`]s and exports *to* vCard -- there's no vCard-file parser to
//! run the other way; `Contact::name`/`fields` is already vCard 3.0's
//! source of truth here, not an intermediate format).
//!
//! Run with: `cargo run --example vcard_parse`

#![warn(clippy::pedantic)]

use vix::org_contacts::{Contact, parse, to_vcard};

fn main() {
    // A real Org-contacts document (`crates/vix-org-contacts/spec/index.md`):
    // one headline per contact, fields in a `:PROPERTIES:` drawer.
    let org = "\
* Alice Example
  :PROPERTIES:
  :EMAIL: alice@example.com
  :PHONE: +1 555 0100
  :BIRTHDAY: 1990-01-15
  :END:

* Bob Sample
  :PROPERTIES:
  :EMAIL: bob@example.com
  :NOTE: Met at the Vix demo workspace
  :END:
";

    let contacts: Vec<Contact> = parse(org);
    for c in &contacts {
        println!("parsed: {} ({} field(s))", c.name, c.fields.len());
    }

    // `to_vcard` takes `(name, content)` pairs, so a real multi-file
    // Org-contacts setup exports as one vCard file.
    let vcf = to_vcard(&[("contacts.org".to_string(), org.to_string())]);
    println!("\n--- contacts.vcf ---\n{vcf}");
}
