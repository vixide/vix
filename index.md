# vixide.github.io

The public site for [Vix](https://github.com/vixide/vix), a keyboard-friendly
terminal text editor — <https://vixide.github.io/>.

A SvelteKit project prerendered with `@sveltejs/adapter-static` and deployed
by GitHub Actions to GitHub Pages. It doesn't implement or ship any part of
the editor itself; it's the landing page, feature tour, and install
instructions.

Styled with [Lily Design System](https://lilydesignsystem.github.io/)'s
headless Svelte components (`lily-design-system-svelte-headless`) — semantic
HTML and ARIA from the library, all CSS hand-written in
[`static/assets/style.css`](static/assets/style.css) against Lily's class
hooks.

## Develop

```sh
npm install
npm run dev       # http://localhost:5173
npm run build     # prerenders to build/
npm run preview   # serve the prerendered build locally
npm run check     # svelte-check
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
