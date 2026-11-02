# File Encodings

Vix detects a file's text encoding when it opens it, and writes the file back
in the same encoding, byte for byte.

## Detection

A byte-order mark wins; then valid UTF-8; then a guess for legacy encodings.
A guess is used only if the file decodes with no malformed sequences **and**
re-encodes to exactly the same bytes. Otherwise the file opens read-only with
unreadable bytes replaced, so detection can never silently corrupt a file.
UTF-16 is always saved with a BOM, since without one it cannot be told from
binary.

The status bar shows the encoding in use (see
[Status panel](../status-panel/index.md)).

## Choosing one

| Action id | Menu item | Does |
| --------- | --------- | ---- |
| `file.reopen_with_encoding` | File → Reopen with Encoding… | Re-reads the file decoded as the encoding you pick. |
| `file.save_with_encoding` | File → Save with Encoding… | Saves the buffer in the encoding you pick. |

The chooser offers 16 labels: UTF-8 (with and without a BOM), UTF-16LE/BE,
windows-1252/1250/1251, ISO-8859-2/15, KOI8-R, GBK, gb18030, Big5, Shift_JIS,
EUC-JP and EUC-KR. A character the chosen encoding cannot hold fails the save;
nothing is ever written as `?`.

See the specification at `crates/vix-encoding/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
