# Jwt Tool

Decode a JSON Web Token's header and payload into readable JSON.

A JWT is `header.payload.signature`, each part `Base64URL` (no padding). This
decodes the first two parts and pretty-prints them; the signature is left
untouched (it cannot be verified without the key). Used by Tools → Convert →
JWT Decode via `App::transform_selection_or_buffer_try`.
