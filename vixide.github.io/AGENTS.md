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
- The header's theme/language/text-size/share controls are one
  `@lilydesignsystem/svelte-picker-bar` `<PickerBar>` (see
  `+layout.svelte`), which composes `svelte-theme-picker`,
  `svelte-locale-picker`, `svelte-text-size-picker`, and `svelte-share-picker`
  internally — those four are transitive dependencies of `svelte-picker-bar`,
  not direct ones; only import them directly if you need one outside the
  bar. `themes`/`sizes` are deliberately left unset in `+layout.svelte` so
  they resolve to `PickerBar`'s own `DEFAULT_THEMES` (all 45 Lily reference
  themes, alphabetical, with the UK/US government themes grouped last —
  don't hand-roll a shorter list) and `DEFAULT_SIZES`. ThemePicker still
  swaps a managed `<link>` between the `static/assets/themes/<slug>.css`
  files (one per `DEFAULT_THEMES` slug, fetched verbatim from
  [`LilyDesignSystem/lily-design-system`'s `themes/`](https://github.com/LilyDesignSystem/lily-design-system/tree/main/themes)
  — never hand-edit one, re-fetch instead) and sets `data-theme` on `<html>`;
  `src/app.html` pre-renders the default `light` `<link>` so the static
  build ships with real CSS in place rather than a picker-injected one.
  LocalePicker's `locales` list is the same 15 languages the Vix editor
  itself ships translations for (`locales/*.yml` in the main `vix` repo) —
  it only sets `lang`/`dir` on `<html>`, there is no page-content
  translation wired up.
- **`page.data.title` convention**: every route's `+page.ts` `load` returns
  `{ title }`; `+layout.svelte` renders the one `<title>` from
  `page.data.title` and passes the same string to `PickerBar`'s
  `shareProps.title`, so a shared link always carries the title of the
  page it was shared from. A page's own `<svelte:head>` should carry
  `<meta name="description">` only — never `<title>`, or it fights the
  layout's.
- Theme colours/radii/fonts (`--color-primary`, `--lily-surface`, etc.)
  live in `static/assets/themes/<slug>.css` — 45 unmodified Lily reference
  theme stylesheets, one per `PickerBar`'s `DEFAULT_THEMES` slug. Don't add
  an application-specific theme file here; `style.css`'s own `:root` block
  only aliases a handful of `--vix-*` names onto those tokens (so the rest
  of the file reads one vocabulary) and owns page-layout tokens that don't
  come from Lily at all (`--vix-content-max`, `--vix-prose-max`).
- Keep feature copy in sync with the `vix` repo's own `index.md` — this site
  paraphrases it, not the other way round; when `vix`'s feature list changes,
  update `src/routes/features/+page.svelte` and the home page's card grid to
  match.
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
