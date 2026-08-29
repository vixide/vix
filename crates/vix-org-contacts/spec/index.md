# Org Contacts

Org-contacts: contact management over Org files
(<https://github.com/doomelpa/org-contacts>).

A *contact* is an ordinary Org headline (its text is the name) whose
`:PROPERTIES:` drawer holds structured fields — `EMAIL`, `PHONE`, `ADDRESS`,
`BIRTHDAY`, `NICKNAME`, `NOTE`, … (the canonical org-contacts property names).
This module is the pure, testable core: a new-contact skeleton, a single
property line, parsing contacts out of Org text, and compiling cross-file
views (a directory listing, a birthday list, and a vCard 3.0 export). The host
(`app`) wires these to the Org → Contacts menu.

All functions are pure so they can be unit-tested without a live editor.

## Property-name case

Vix **writes** the canonical org-contacts spelling — uppercase drawer keys
(`:EMAIL:`, `:PHONE:`, …) — and **reads** case-insensitively, so a file written
by Emacs org-contacts with lowercase keys (`:email:`) parses the same way. Both
spellings are therefore valid input; new contacts Vix creates use uppercase.

## Capture template

The bundled Contact capture template (Org → Capture, see
[org-capture spec](../../vix-org-capture/spec/)) files a headline shaped like
this — the name is the headline text, everything else is a drawer field:

```org
* Alice Adams
  :PROPERTIES:
  :EMAIL:
  :PHONE:
  :ADDRESS:
  :BIRTHDAY:
  :NICKNAME:
  :NOTE:
  :END:
```

Extra, non-canonical fields (`:COMPANY:`, `:TITLE:`, `:GITHUB_URL:`, …) are
preserved verbatim by the parser; only the fields in `FIELDS` drive the
directory, birthday, and vCard views.

## See also

- [contact-panel spec](../../vix-contact-panel/spec/) — shared contacts model
