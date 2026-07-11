# Vcard Parser

A small, dependency-free vCard 4.0 ([RFC 6350]) parser.

[`parse`] turns vCard text into a [`Vcard`] — a flat list of [`Property`]s,
each with a name, parameters, and an unescaped value. It handles the parts of
the grammar that matter for displaying a contact: line **unfolding** (a line
starting with a space or tab continues the previous one), the
`name;PARAM=value:VALUE` shape (including group prefixes like `item1.EMAIL`
and legacy bare `TYPE` parameters), and value **unescaping** (`\\`, `\n`,
`\,`, `\;`). It is pure: the host reads the `.vcf` files.

[RFC 6350]: https://www.rfc-editor.org/info/rfc6350

## See also

- [contact-panel spec](../../vix-contact-panel/spec/) — shared contacts model
