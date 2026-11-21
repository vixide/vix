# License

Vix™ and Vix IDE™ source code and documentation are licensed under any one of these, at the recipient's option, or contact us for custom license options.

* [MIT](https://opensource.org/license/mit) ([SPDX: MIT](https://spdx.org/licenses/MIT.html))

* [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0) ([SPDX: Apache-2.0](https://spdx.org/licenses/Apache-2.0.html))

* [GNU General Public License v2.0](https://www.gnu.org/licenses/old-licenses/gpl-2.0-standalone.html) ([SPDX: GPL-2.0-only](https://spdx.org/licenses/GPL-2.0-only.html))

* [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0-standalone.html) ([SPDX: GPL-3.0-only](https://spdx.org/licenses/GPL-3.0-only.html))

* [BSD 3-Clause License](https://opensource.org/license/bsd-3-clause) ([SPDX: BSD-3-Clause](https://spdx.org/licenses/BSD-3-Clause.html))

* [UK Open Government Licence v3.0](https://www.nationalarchives.gov.uk/doc/open-government-licence/version/3/) ([SPDX: OGL-UK-3.0](https://spdx.org/licenses/OGL-UK-3.0.html))

* [Creative Commons Attribution Non Commercial Share Alike 4.0 International](https://creativecommons.org/licenses/by-nc-sa/4.0/) ([SPDX: CC-BY-NC-SA-4.0](https://spdx.org/licenses/CC-BY-NC-SA-4.0.html))

```
SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-2.0-only OR GPL-3.0-only OR OGL-UK-3.0 OR CC-BY-NC-SA-4.0
```

## Distributed binaries

A distributed binary also contains every third-party crate it links, so each binary is offered only under the licenses that all of its linked code allows. The two newest licenses (OGL-UK-3.0 and CC-BY-NC-SA-4.0) apply to Vix's own source code and documentation, not to binaries, and each binary's license is unchanged:

* `vix` (`vix-tui`), and the `vix-gpui`, `vix-egui`, `vix-iced`, `vix-gtk4`, and `vix-zgui` front ends: Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-3.0-only.

* `vix-slint` and `vix-bui`: GPL-3.0-only, because their frameworks require it.

See [`spec/license/index.md`](spec/license/index.md) and [`spec/licenses/index.md`](spec/licenses/index.md). The license text for each binary is the `LICENSE` file in its package directory.

Vix™ and Vix IDE™ are trademarks.

Copyright © 2026 Vix™ and Vix IDE™.

## Full license texts

The full text of each license is in its own directory:

- [MIT](LICENSES/mit/index.md)
- [Apache-2.0](LICENSES/apache-2.0/index.md)
- [BSD-3-Clause](LICENSES/bsd-3-clause/index.md)
- [GPL-2.0-only](LICENSES/gpl-2.0-only/index.md)
- [GPL-3.0-only](LICENSES/gpl-3.0-only/index.md)
- [OGL-UK-3.0](LICENSES/ogl-uk-3.0/index.md)
- [CC-BY-NC-SA-4.0](LICENSES/cc-by-nc-sa-4.0/index.md)
