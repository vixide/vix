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

- Uses [Lily Design System](https://lilydesignsystem.com/)'s headless
  Svelte components (`@lilydesignsystem/svelte-headless`) for semantic
  HTML/ARIA structure — `Header`, `NavigationMenu`, `Card`, etc. They ship no
  CSS; every rule lives in [`static/assets/style.css`](static/assets/style.css)
  against Lily's class hooks (`.button`, `.card`, `.header`, `.navigation-menu`,
  …). Keep new UI on this pattern rather than hand-rolled markup where a
  matching headless component exists.
- The header's search/theme/language/text-size/share controls are one
  `@lilydesignsystem/svelte-picker-bar` `<PickerBar>` (see
  `+layout.svelte` and [index.md](index.md) "Header: PickerBar"), which
  composes the individual `svelte-*-picker` packages internally — those are
  transitive dependencies of `svelte-picker-bar`, not direct ones; only
  import one directly if you need it outside the bar. `themes`/`sizes` are
  deliberately left unset so they resolve to `PickerBar`'s own
  `DEFAULT_THEMES` (all 57 Lily reference themes, alphabetical, with the
  UK/US government themes grouped last — don't hand-roll a shorter list)
  and `DEFAULT_SIZES`. The search box sends its query to the mdBook docs at
  `https://vixide.github.io/vix/?search=…`. ThemePicker swaps a managed
  `<link>` between the `static/assets/themes/<slug>.css` files and sets
  `data-theme` on `<html>`; `src/app.html` pre-renders the default `light`
  `<link>` so the static build ships with real CSS in place.
  LocalePicker's `locales` list is the 15 languages the Vix editor is fully
  translated into — it only sets `lang`/`dir` on `<html>`, there is no
  page-content translation wired up.
- **Themes come from the `@lilydesignsystem/themes` package**, never
  vendored: `scripts/copy-lily-themes.js` copies its `dist/*.css` into the
  gitignored `static/assets/themes/` on every `dev`/`build`. Never commit or
  hand-edit a theme file; upgrade the package instead.
- **`page.data.title` convention**: every route's `+page.ts` `load` returns
  `{ title }`; `+layout.svelte` renders the one `<title>` from
  `page.data.title` and passes the same string to `PickerBar`'s
  `shareProps.title`, so a shared link always carries the title of the
  page it was shared from. A page's own `<svelte:head>` should carry
  `<meta name="description">` only — never `<title>`, or it fights the
  layout's.
- Theme colours/radii/fonts (`--color-primary`, `--lily-surface`, etc.)
  live in the theme stylesheets. Don't add an application-specific theme
  file; `style.css`'s own `:root` block only aliases a handful of `--vix-*`
  names onto those tokens (so the rest of the file reads one vocabulary)
  and owns page-layout tokens that don't come from Lily at all
  (`--vix-content-max`, `--vix-prose-max`). The themes style Lily's
  component class hooks (`.card`, `.header`, `.hero`, the `*-picker`
  parts, …) in `@layer lily`; give site-only blocks a `site-` prefix (as
  `.site-hero` does) so a theme's rule for a same-named Lily hook can't
  leak in.
- Demo GIFs in `static/assets/demos/` are verbatim copies of the main
  repo's `docs/demos/*.gif`; re-copy them when those are re-rendered.
- Keep feature copy in sync with the `vix` repo's own `index.md` — this site
  paraphrases it, not the other way round; when `vix`'s feature list changes,
  update `src/routes/features/+page.svelte` and the home page's card grid to
  match. Verify every claim against the repo; don't invent numbers.
- Package manager is **pnpm** (see `packageManager` in `package.json`), not
  npm — `pnpm install`, `pnpm run build`, `pnpm run check`; the deploy
  workflow (`.github/workflows/deploy.yml`) uses `pnpm/action-setup` and
  `pnpm install --frozen-lockfile`.
- `pnpm run build` must prerender cleanly (`strict: true` in
  `svelte.config.js` fails the build on an unreachable route or broken link)
  before pushing to `main` — that's also what the deploy workflow runs.
- Run `pnpm run check` (svelte-check) before committing changes to `src/`.

---

Vix™ and Vix IDE™ are trademarks.
