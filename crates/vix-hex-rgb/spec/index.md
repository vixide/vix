# Hex RGB

Lenient `#RRGGBB`/`RRGGBB` hex-string to `(r, g, b)` byte-triple parsing,
treating any missing or invalid component as `0`.

**Status:** Shipped (Run H, T530). One pure function:
`rgb(hex: &str) -> (u8, u8, u8)`.

## Why this crate exists

`vix-editor-core` (syntax-highlight color parsing) and `vix-base16`
(rendering a base16 palette's hex strings into theme JSON) each hand-rolled
an independent copy of the same lenient hex parser — same policy (strip a
leading `#`, treat any missing/invalid two-character component as `0`
rather than reject the whole string), same shape, real duplication rather
than domain-driven similarity. Neither otherwise depends on a hex/color
parsing library, and neither wants `vix-color-converter-tool::from_hex`'s
behavior: that's a deliberately *stricter* public-facing API (used by the
Tools → Color Converter panel), which rejects malformed input instead of
zero-filling it — a real, intentional difference in policy, not something
to unify away.

## API

```rust
assert_eq!(vix_hex_rgb::rgb("#F0F8FF"), (0xF0, 0xF8, 0xFF));
assert_eq!(vix_hex_rgb::rgb("f0f8ff"), (0xF0, 0xF8, 0xFF));
// Missing or invalid components are 0, not an error.
assert_eq!(vix_hex_rgb::rgb("f0"), (0xF0, 0, 0));
assert_eq!(vix_hex_rgb::rgb("zzzzzz"), (0, 0, 0));
```

## Consumers

- `vix-editor-core`: parsing a syntax theme's `#RRGGBB` color strings.
- `vix-base16`: rendering a bundled `Palette`'s hex strings into the
  `[r, g, b]` JSON arrays the theme schema expects.
