# IDE parity assessment: Visual Studio and JetBrains

**Status:** assessment and advice (2026-10-28); T720–T725 and T727 implemented, T726 (CodeLens) partial (see [docs](../../docs/ide-parity/index.md)). Tasks T720–T729 in
[`tasks.md`](../../tasks.md).

*Sources: the author's knowledge of Visual Studio, IntelliJ-platform IDEs
(IntelliJ IDEA, RustRover, etc.) and Vix's own action catalog. No live web
research. The "Vix today" column comes from a quick grep of
`docs/reference/actions.md` and is a signal, not an audit: confirm before
starting any task.*

## How to read this

**Fit** rates how well the feature suits a keyboard-driven terminal editor:
**High** (do it), **Medium** (do it when cheap), **Low** (a GUI-centric
feature; skip or leave to external tools). Vix's positioning is fast,
specification-driven and terminal-native, not a clone of a 2 GB IDE.

## Feature inventory and assessment

| Feature (VS / JetBrains) | Vix today | Fit | Advice |
| ------------------------ | --------- | --- | ------ |
| **Local History** (JetBrains: automatic per-file snapshots independent of git, label, revert a range) | persistent undo only | covered | Biggest differentiator JetBrains users miss. Build on `vix-undo-store`: timestamped snapshots on save plus a diff view. (T720) Done. |
| **Search Everywhere** (double-Shift: files, symbols, actions, settings in one box) | palette, file finder, symbol finder separately | covered | One fuzzy box with prefixes (`>` action, `@` symbol, `:` line). (T721) Done. |
| **Refactor: rename, extract, inline, move, change signature** | LSP rename; some code actions | covered | Surface every LSP `codeAction` of kind `refactor.*` in a Refactor menu with preview. (T722) Done. |
| **Optimize/organize imports, code cleanup, reformat on commit** | format; no organize imports | covered | LSP `source.organizeImports`, plus a "cleanup" pipeline. (T723) Done. |
| **Run configurations** (named, shareable, compound, before-launch steps) | tasks | covered | Extend `vix-tasks` with named, saved configs per project. (T724) Done. |
| **Debugger depth** (conditional/log breakpoints, watches, evaluate, data breakpoints) | basic DAP | **High** | Already tracked as T617; link, don't duplicate. |
| **TODO tool window** | TODO Finder | **High** | Becomes the Todo Tree (T682). |
| **Structure view / outline, breadcrumbs** | outline panel, breadcrumbs | covered | none |
| **Bookmarks with mnemonics, numbered** | present | covered | none |
| **Call/type hierarchy** | call hierarchy only | **Medium** | Add type hierarchy: tracked as T620. |
| **Peek definition** (VS), **Quick Definition** (JetBrains) | none | covered | An inline read-only popup of the target; cheap with LSP. (T725) Done. |
| **Inlay hints, Code Vision / CodeLens** | inlay hints present; lenses in a line-numbered chooser | **Medium** | CodeLens (references, run test) is a thin layer over LSP. (T726) Partial: lenses are listed, not yet drawn inline above their lines. |
| **Postfix completion and live templates** | snippets | covered | Add postfix (`.if`, `.for`) on top of the snippet engine. (T727) Done. |
| **Changelists and Shelve** (JetBrains) | git stash | **Medium** | Map to named stashes plus a changelist view; lower than Local History. |
| **Interactive rebase UI, annotate/blame, 3-way merge** | blame, rebase, merge tool present | covered | Polish only. |
| **Services tool window** (Docker, DB, servers) | DB panel; OC planned | **High** | Realized by the OC menu (T700+). |
| **Remote Development / Gateway / Remote-SSH** | none | **High** | Own spec: [remote-editing](../remote-editing/index.md). |
| **Code With Me / Live Share** | none | **High** | Own spec: [cowork](../cowork/index.md). |
| **HTTP client** (JetBrains `.http` files) | basic | **High** | T680 brings it to parity. |
| **Database tools** | present | covered | none |
| **Test runner with gutter icons, coverage** | present | covered | none |
| **Profiler, memory/perf tools, snapshot debugging** | none | **Low** | Launch external profilers from a task; don't build. |
| **GUI designers (WinForms/XAML), UML diagrams, decompiler** | none | **Low** | Out of scope for a terminal editor. |
| **NuGet / package-manager UI, solution explorer** | project types in `vix-tasks` | **Medium** | A dependency view reading `Cargo.toml`/`package.json` is plausible later. |
| **Settings sync, presentation mode, scratch files** | scratch and zen present | **Low** | Settings export exists (T555); skip sync. |
| **IntelliCode / AI ranking** | AI provider support | covered | Tracked under inline completion (T618). |

## Advice

1. **Do the cheap High items first:** Search Everywhere (T721), Run
   configurations (T724), organize imports (T723). Each reuses an existing
   crate and adds no dependency.
2. **Local History (T720) is the one JetBrains feature with real pull** and no
   good terminal-editor equivalent. Worth the investment.
3. **Refactor menu (T722) is mostly UI over LSP**; quality depends on the
   server, so test with `rust-analyzer` and one other.
4. **Treat the two big items as products, not features:** Cowork and Remote
   editing each have their own spec and phases.
5. **Say no to the Low rows in the docs** so users know it's deliberate.
6. Re-scan this table when a task here lands; mark the row "covered".
