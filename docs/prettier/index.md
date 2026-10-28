# Format with Prettier

Vix™ can format JavaScript, TypeScript, JSON, CSS, HTML, Markdown, YAML,
GraphQL and Handlebars files with your own [Prettier](https://prettier.io).
Vix never bundles Node or Prettier: install it yourself.

## Turn it on

Prettier is opt-in per language. List the languages in `config.toml`:

```toml
prettier_languages = ["typescript", "css", "markdown"]
```

The language ids are `javascript` (`.js .jsx .mjs .cjs`), `typescript`
(`.ts .tsx .mts .cts`), `json` (`.json .jsonc .json5`), `css` (`.css .scss
.less`), `html` (`.html .htm .vue`), `markdown` (`.md .markdown .mdx`), `yaml`,
`graphql` and `handlebars`.

## Use it

For a listed language, **Format Document** and
`format_on_save` run `prettier --stdin-filepath <file>` instead of the language
server. Files in any other language still use the language server.

Prettier reads your `.prettierrc*`, `.editorconfig` and `.prettierignore`
itself, so the result matches what `prettier --write` gives you. An ignored
file is left unchanged.

## Which Prettier runs

1. The nearest `node_modules/.bin/prettier`, walking up from the file.
2. The first `prettier` on your `PATH`.

A project-local Prettier is code from the repository, so Vix uses it only in a
workspace you have trusted (the same decision that gates project scripts). In
an untrusted workspace only the `PATH` Prettier runs.

## When it cannot run

The buffer is left unchanged and the status line says why: Prettier was not
found (with how to install it), it timed out after 10 seconds, or it failed
(usually a syntax error, with Prettier's message).
