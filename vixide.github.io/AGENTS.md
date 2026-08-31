# AGENTS.md

Guidance for AI agents and human contributors working in this repository.

## What this is

A SvelteKit project (`@sveltejs/adapter-static`) that prerenders the public
site for [Vix](https://github.com/vixide/vix), deployed by GitHub Actions to
<https://vixide.github.io/>. It does not implement or ship any part of the
editor — see [index.md](index.md) for scope.

## Metadata

- **Package**: vixide.github.io
- **License**: Apache-2.0 or BSD-3-Clause or MIT or GPL-2.0 or GPL-3.0
- **Contact**: Joel Parker Henderson (joel@joelparkerhenderson.com)

## Working rules

- Uses [Lily Design System](https://lilydesignsystem.github.io/)'s headless
  Svelte components (`lily-design-system-svelte-headless`) for semantic
  HTML/ARIA structure — `Header`, `NavigationMenu`, `Card`, etc. They ship no
  CSS; every rule lives in [`static/assets/style.css`](static/assets/style.css)
  against Lily's class hooks (`.button`, `.card`, `.header`, `.navigation-menu`,
  …). Keep new UI on this pattern rather than hand-rolled markup where a
  matching headless component exists.
- Also uses three Lily Svelte helpers, same headless-plus-hand-written-CSS
  pattern: `lily-design-system-svelte-theme-picker` (site light/dark toggle
  in the header — attribute-based, swaps a managed `<link>` between
  [`static/assets/themes/light.css`](static/assets/themes/light.css) and
  [`dark.css`](static/assets/themes/dark.css) and sets `data-theme` on
  `<html>`; `src/app.html` pre-renders the default `light` `<link>` so the
  static build ships with real CSS in place rather than a picker-injected
  one), `lily-design-system-svelte-share-picker` (header share button; see
  the `page.data.title` convention below for how it gets its title), and
  `lily-design-system-svelte-text-size-picker` (installed, not yet wired
  into a page).
- **`page.data.title` convention**: every route's `+page.ts` `load` returns
  `{ title }`; `+layout.svelte` renders the one `<title>` from
  `page.data.title` and passes the same string to SharePicker, so a shared
  link always carries the title of the page it was shared from. A page's own
  `<svelte:head>` should carry `<meta name="description">` only — never
  `<title>`, or it fights the layout's.
- All theme colours (`--vix-primary`, `--vix-page-bg`, etc.) live in
  `static/assets/themes/{light,dark}.css`, not in `style.css` — `style.css`
  keeps only the tokens that don't vary by theme (radius, spacing, fonts)
  plus every rule that consumes theme tokens via `var()`. Adding a colour
  means adding the same variable to both theme files.
- Keep feature copy in sync with the `vix` repo's own `index.md` — this site
  paraphrases it, not the other way round; when `vix`'s feature list changes,
  update `src/routes/features/+page.svelte` and the home page's card grid to
  match.
- `npm run build` must prerender cleanly (`strict: true` in
  `svelte.config.js` fails the build on an unreachable route or broken link)
  before pushing to `main` — that's also what the deploy workflow runs.
- Run `npm run check` (svelte-check) before committing changes to `src/`.

---

Vix™ and Vix IDE™ are trademarks.
