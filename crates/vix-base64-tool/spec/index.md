# Base64

Base64 encode and decode (Tools → Convert → Base64). Module `base64_tool`.

- menu "Tools"
  - submenu "Convert"
    - submenu "Base64"
      - menuitem "Encode"
      - menuitem "Decode"

Convert the selected text (or if no selected text then the entire buffer) via encode/decode base64.

## Errors (T610)

`Base64Error`; `encode`/`decode` return `Result<String, Base64Error>` (`encode` never fails):

| Variant | Meaning |
| --- | --- |
| `Decode(base64::DecodeError)` | the input is not valid standard Base64 |
| `NotUtf8(FromUtf8Error)` | the decoded bytes are not UTF-8 text |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `status.convert_failed` line is unchanged.
