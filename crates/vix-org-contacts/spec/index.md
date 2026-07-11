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

## See also

- [contact-panel spec](../../vix-contact-panel/spec/) — shared contacts model
