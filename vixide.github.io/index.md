# vixide.github.io

The public site for [Vix](https://github.com/vixide/vix), a keyboard-friendly
terminal text editor and IDE — <https://vixide.github.io/>.

A SvelteKit project prerendered with `@sveltejs/adapter-static` and deployed
by GitHub Actions to GitHub Pages. It doesn't implement or ship any part of
the editor itself; it's the landing page, feature tour, and install
instructions.

Styled with [Lily Design System](https://lilydesignsystem.com/)'s
headless Svelte components (`@lilydesignsystem/svelte-headless`) — semantic
HTML and ARIA from the library, page CSS hand-written in
[`static/assets/style.css`](static/assets/style.css) against Lily's class
hooks and tokens. See [AGENTS.md](AGENTS.md) for the theme-file and
page-title conventions.

## Audience and content

The site is for a visitor deciding in about 30 seconds whether to try Vix.
The home page leads with what Vix is (a terminal text editor and IDE in
Rust, with familiar Ctrl/Cmd keys and a menu bar rather than modal editing
by default), a copyable install command, a real screenshot, a six-card
"Why Vix" grid, recorded demos, the keys to start with, links to the
documentation's `for-*-users` guides and feature comparison, the desktop
GUIs, and the open-source/licensing facts. `/features/` is the grouped
feature list with an on-page contents nav and every demo; `/install/` has
every install route (Homebrew, installer script, PowerShell, npm, cargo,
direct download, the minimal build, the desktop GUIs, building from source)
and a first-run section. Every claim is checked against the main `vix`
repo (its `index.md`, `docs/`, crate specs and `CHANGELOG.md`); no number or
feature appears here that the repo doesn't state.

Screenshots and demos are the main repo's `docs/demos/*.gif` recordings,
copied verbatim into `static/assets/demos/` (the site is exported as its
own repository, so it can't read them from `../docs` at build time).
Every image carries `width`/`height` and alt text; all but the hero image
load lazily.

Install commands are shown by `src/lib/CopyCommand.svelte`: a code block
plus Lily's headless `ClipboardCopyButton`, with a polite live region that
announces "Copied".

## Header: PickerBar

The header's controls are one `@lilydesignsystem/svelte-picker-bar`
`PickerBar` (0.5), which renders, in order:

- **Search** — searches the Vix documentation, the mdBook build of the main
  repo's `docs/` published at <https://vixide.github.io/vix/>. The query is
  sent as mdBook's own `?search=` parameter, so it opens real results; the
  site builds no search index of its own.
- **Theme** — all 57 Lily reference themes (`DEFAULT_THEMES`).
- **Language** — the 15 languages the Vix editor is fully translated into.
  It sets `lang`/`dir` on `<html>` only; page content isn't translated.
- **Text size** — Lily's seven-step scale (`DEFAULT_SIZES`).
- **Share** — Email, LinkedIn, Reddit, Bluesky, Mastodon, and Copy Link.

The bar's optional link picker and time-zone picker are not used: the nav
beside it already lists every page, and the site shows no times.

## Themes

The theme stylesheets come from the `@lilydesignsystem/themes` npm package
rather than vendored copies. `scripts/copy-lily-themes.js` (run by `pnpm
run dev` and `pnpm run build`, or alone as `pnpm run themes`) copies the
package's `dist/*.css` into `static/assets/themes/`, which is gitignored,
so the site always serves exactly the installed package's themes and a
theme update is a version bump. The package's 57 files match PickerBar's
`DEFAULT_THEMES` slug for slug. Each theme also styles Lily's own
components (including every picker) in `@layer lily`, so `style.css` no
longer styles the pickers itself beyond laying out the bar.

## Develop

```sh
pnpm install
pnpm run dev       # http://localhost:5173
pnpm run build     # copies the Lily themes, then prerenders to build/
pnpm run preview   # serve the prerendered build locally
pnpm run check     # svelte-check
```

## Routes

- `/` — home
- `/features/` — the grouped feature list and every demo
- `/install/` — every install option, the minimal build, desktop GUIs, and first run

## License

Licensed under your choice of Apache-2.0, BSD-3-Clause, MIT, GPL-2.0, or
GPL-3.0 — see [LICENSE](LICENSE).

---

Vix™ and Vix IDE™ are trademarks.
