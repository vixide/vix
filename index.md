# vixide.github.io

The public site for [Vix](https://github.com/vixide/vix), a keyboard-friendly
terminal text editor — <https://vixide.github.io/>.

A SvelteKit project prerendered with `@sveltejs/adapter-static` and deployed
by GitHub Actions to GitHub Pages. It doesn't implement or ship any part of
the editor itself; it's the landing page, feature tour, and install
instructions.

Styled with [Lily Design System](https://lilydesignsystem.com/)'s
headless Svelte components (`@lilydesignsystem/svelte-headless`) — semantic
HTML and ARIA from the library, all CSS hand-written in
[`static/assets/style.css`](static/assets/style.css) against Lily's class
hooks. The header's theme, language, text-size, and share controls are one
`@lilydesignsystem/svelte-picker-bar` `PickerBar`, offering all 45 Lily
reference themes and the 15 languages the Vix editor itself ships
translations for — see [AGENTS.md](AGENTS.md) for the theme-file and
page-title conventions.

## Develop

```sh
pnpm install
pnpm run dev       # http://localhost:5173
pnpm run build     # prerenders to build/
pnpm run preview   # serve the prerendered build locally
pnpm run check     # svelte-check
```

## Routes

- `/` — home
- `/features/` — the full feature list
- `/install/` — build and run from source

## License

Licensed under your choice of Apache-2.0, BSD-3-Clause, MIT, GPL-2.0, or
GPL-3.0 — see [LICENSE](LICENSE).

---

Vix™ and Vix IDE™ are trademarks.
