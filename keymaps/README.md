# Reference keymaps

Other editors' default keybindings, kept as reference material for designing
and checking Vix's own keymaps. Nothing in the build reads these files: Vix's
keymaps are hand-written tables in
[`crates/vix-keybindings`](../crates/vix-keybindings/spec/index.md), and the
user-facing list is [`docs/keymaps/index.md`](../docs/keymaps/index.md).

`tui/tests/keymaps_reference.rs` checks that every `*.json` file here parses as
a VS Code keybindings array (JSON with `//` comments), so a bad download (for
example a saved web page instead of the raw file) fails the gate.

## Visual Studio Code

| Folder | Upstream file |
| --- | --- |
| `visual-studio-code-for-linux/` | `linux.keybindings.json` |
| `visual-studio-code-for-macos/` | `macos.keybindings.json` |
| `visual-studio-code-for-windows/` | `windows.keybindings.json` |

- **Contents:** the default keybindings of Visual Studio Code 1.144.0, as
  VS Code itself lists them ("Preferences: Open Default Keyboard Shortcuts
  (JSON)"), one file per operating system.
- **Source:** the `codebling/vs-code-default-keybindings` repository on
  GitHub, which regenerates the files from each VS Code release with a GitHub
  Actions workflow. Downloaded from the raw file URLs,
  `https://raw.githubusercontent.com/codebling/vs-code-default-keybindings/master/<os>.keybindings.json`,
  at upstream commit `306922924cf8cc32538989a54557ead9c36fac88` ("Update for
  VSCode 1.144.0").
- **Downloaded:** 2026-10-30. Byte-for-byte as published, except the Windows
  file's CRLF line endings are normalized to LF (the repo's
  `.gitattributes` does this on commit anyway).
- **License:** the upstream repository declares no license. The data is
  VS Code's own default-keybinding list (Microsoft), kept here unmodified as
  reference only; it is not compiled into, linked by, or shipped with any Vix
  binary.

To refresh, download the three raw URLs above again, update the version,
commit, and date in this file, and run
`cargo test -p vix-tui --test keymaps_reference`.
