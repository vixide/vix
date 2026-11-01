# URL

URL percent-encode and decode (Tools → Convert → URL). This is module
`url_tool`.

- menu "Tools"
  - submenu "Convert"
    - submenu "URL"
      - menuitem "Encode"
      - menuitem "Decode"

Convert the selected text (or if no selected text then the entire buffer) via URL encode or URL decode.

## Errors (T610)

`UrlError`; `encode`/`decode` return `Result<String, UrlError>` (`encode` never fails):

| Variant | Meaning |
| --- | --- |
| `NotUtf8(Utf8Error)` | the percent-decoded bytes are not UTF-8 text |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `status.convert_failed` line is unchanged.
