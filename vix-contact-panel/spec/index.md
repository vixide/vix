# Contact Panel

A browser for a directory of vCard files. **Tools → Contacts…** scans the
configured directory for `.vcf` files and lists them as a table of contact names;
choosing one opens that contact's [vCard view](../vix-vcard-panel/spec/index.md).

## As implemented in Vix

**Status:** Shipped. The `vix-contact-panel` crate holds the contact list
(`Contact { name, path }`) and the row-selection + scroll state; it is pure data.
The host (`src/app.rs`, `src/ui.rs`) scans the directory, parses each file's
display name with `vix-vcard-parser`, sorts the contacts by name, and renders the
overlay.

**Directory.** The `contacts_dir` setting names the vCard directory; when empty,
the workspace root is scanned. Non-`.vcf` files are ignored; a file with no `FN`
falls back to its filename. If no vCards are found, a status note says so.

| Key / action  | Effect                                          |
| ------------- | ----------------------------------------------- |
| `↑` / `↓`     | Move the highlight                              |
| `PgUp`/`PgDn` | Move one page                                   |
| `Home`/`End`  | Jump to the first / last contact                |
| `Enter` / click | Open the highlighted contact's vCard          |
| `Esc`         | Close the browser                               |

The list has a scrollbar when it overflows.
