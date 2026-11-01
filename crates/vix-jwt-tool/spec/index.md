# Jwt Tool

Decode a JSON Web Token's header and payload into readable JSON.

A JWT is `header.payload.signature`, each part `Base64URL` (no padding). This
decodes the first two parts and pretty-prints them; the signature is left
untouched (it cannot be verified without the key). Used by Tools → Convert →
JWT Decode via `App::transform_selection_or_buffer_try`.

## Errors (T610)

`JwtError`; `decode` returns `Result<String, JwtError>`:

| Variant | Meaning |
| --- | --- |
| `NotJwt` | fewer than two non-empty dot-separated parts |
| `Base64(DecodeError)` | a part is not valid Base64URL |
| `NotUtf8(FromUtf8Error)` | a part decoded to non-UTF-8 bytes |
| `Json { message }` | a part decoded to text that is not JSON |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `status.convert_failed` line is unchanged.
