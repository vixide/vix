# License

Vix™ and Vix IDE™ are licensed under any of:
- Apache 2.0
- BSD-3 Clause
- MIT
- GPL 2.0-only
- GPL 3.0-only

Vix™ and Vix IDE™ are trademarks.

The license texts are in [`docs/licenses/`](../../docs/licenses/).

## Binary licenses (T648)

The five licenses above apply to Vix's **source code**: every crate under
`crates/`, the App shell in `tui/`, and each GUI front end's own code. A
**distributed binary** also contains every third-party crate it links, so it
can be offered only under those of the five that all of its linked code
allows. Each binary therefore respects the licenses of its framework (the
toolkit it is built on) and of the rest of its dependency graph:

- **Each binary package declares its own `license`**: exactly the subset of
  the five that its linked dependencies permit — not more (that would offer
  terms the binary cannot honor), not fewer (that would withhold a choice
  for no reason). The packages are `vix-tui` (the `vix` command) and each
  desktop front end in `gui/`.
- **Each binary package ships its own `LICENSE`** (`tui/LICENSE`,
  `gui/<front end>/LICENSE`) stating that subset and why, which `dist`
  bundles into that binary's archives and installers in place of the
  repo-root `LICENSE`.
- **How a dependency narrows the set.** Permissive licenses (MIT, BSD,
  ISC, Zlib, BSL-1.0, Unicode, CC0, …), MPL-2.0 (file-level copyleft that
  does not reach the linking work) and font licenses that allow bundling
  (OFL-1.1, Ubuntu Font) leave all five. **Apache-2.0 with no alternative**
  rules out GPL-2.0-only (the FSF holds the two incompatible) but is
  compatible with GPL-3.0-only. A **GPL-3.0-only** dependency allows only
  GPL-3.0-only. A proprietary (`LicenseRef-`) alternative allows nothing.
  An `OR` is a choice (the union); an `AND` is two obligations (the
  intersection). System libraries linked dynamically and not shipped in the
  binary (GTK 4 and its stack, LGPL-2.1-or-later) do not narrow it.
- **Enforced** by `scripts/check-binary-licenses` (run by `scripts/check`
  and CI on all three forges): it walks each binary's linked dependency
  graph across all target platforms (normal dependencies only; proc-macros
  and build-only dependencies are not in the binary), computes the set, and
  fails when a declared `license` differs, naming the crates that rule each
  license out. A license id it has not classified also fails, so a new kind
  of license is reviewed, not guessed.

| Binary | Framework (license) | Binary license | Narrowed by |
| --- | --- | --- | --- |
| `vix` (`vix-tui`) | ratatui (MIT) | Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only | Apache-2.0-only crates (`approx`, `gethostname`, `htmd`, `ring`, `similar`) |
| `vix-gpui` | GPUI Kit, GPUI (Apache-2.0) | Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only | the framework itself, plus the above |
| `vix-egui` | egui, eframe (MIT OR Apache-2.0) | Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only | Apache-2.0-only crates (`winit`, `glutin`, `ab_glyph`, …) |
| `vix-iced` | iced (MIT) | Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only | Apache-2.0-only crates (`winit`, `ab_glyph`, …) |
| `vix-gtk4` | gtk4-rs (MIT); GTK 4 (LGPL-2.1-or-later, dynamic) | Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only | Apache-2.0-only crates (as `vix`) |
| `vix-slint` | Slint (GPL-3.0-only, or Slint's royalty-free / commercial licenses) | GPL-3.0-only | the framework, used under its GPLv3 option |

The table is derived, not authoritative: `scripts/check-binary-licenses`
recomputes it from the lockfiles, and a dependency change that alters a
binary's set fails the gate until the package's `license` and `LICENSE`
are updated to match. The library crates under `crates/` are source-only
packages and keep all five licenses.
