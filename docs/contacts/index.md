# Contacts

Vix has two kinds of contacts: a read-only browser for a folder of vCard
(`.vcf`) files, and Org-contacts entries that live as headlines in your
`.org` files.

## vCard browser: Tools → Contacts…

<!-- Facts: tui/src/app.rs (open_contacts); tui/src/app/info_panels.rs
(contacts_key, vcard_key, insert_selected_vcard_value);
crates/vix-contact-panel/spec/index.md. -->

**Tools → Contacts…** lists every `.vcf` file in the `contacts_dir` setting's
folder, or in the workspace root when `contacts_dir` is empty. Each row is the
card's `FN` name (else the file name), sorted alphabetically. If the folder
has no vCard files, the status line says so and the list is empty.

| Key | In the list | In a card |
| --- | --- | --- |
| `↑` / `↓`, `PgUp` / `PgDn`, `Home` / `End` | Move the highlight | Move the highlight |
| `Enter` or a click | Open the highlighted card | Insert the highlighted value at the cursor |
| `Esc` | Close the browser | Back to the list |

A card shows its fields with readable labels, such as **Email (work)** or
**Phone**. Vix only reads `.vcf` files; it never changes them. The full
behavior is in [`crates/vix-contact-panel/spec/index.md`](../../crates/vix-contact-panel/spec/index.md).

## Org contacts: Org → Contacts

<!-- Facts: crates/vix-org-contacts/src/lib.rs (FIELDS, parse, directory,
birthdays, to_vcard); tui/src/app.rs (contacts_action, contacts_buffer);
tui/src/app/roam.rs (roam_node_files); tui/src/app/org.rs
(maybe_complete_org_contact_link); crates/vix-menu/src/lib.rs (ORG_CONTACTS). -->

An Org contact is a headline whose `:PROPERTIES:` drawer has at least one of
`EMAIL`, `PHONE`, `ADDRESS`, `BIRTHDAY`, `NICKNAME` or `NOTE`, as in
Emacs org-contacts. Vix reads the property names in any case.

```org
* Alice Adams
  :PROPERTIES:
  :EMAIL: alice@example.com
  :PHONE: +1 555 0100
  :END:
```

- **Org → Capture → Contact…** asks for a name, email, phone, address,
  birthday, nickname and note, and inserts an entry like the one above.
- **Org → Contacts → Insert Field** inserts one `:EMAIL:`, `:PHONE:`, … line
  at the cursor.
- **Find Contacts**, **Birthdays** and **Export to vCard** read every `.org`
  file in the project and open the result in a new buffer: a table of names,
  emails and phones; a list of birthdays sorted by date; or vCard 3.0 text.
- **Complete Link…**, with the cursor right after `[[mailto:` or
  `[[contact:`, offers email addresses or contact names.

See [Org mode](../org/index.md) for capture templates and the rest of the Org
menu, and [`crates/vix-org-contacts/spec/index.md`](../../crates/vix-org-contacts/spec/index.md)
for the details.

---

Vix™ and Vix IDE™ are trademarks.
