<svelte:head>
  <meta name="description" content="The full feature list for the Vix terminal text editor." />
</svelte:head>

<section class="section prose" style="margin: 0 auto;">
  <h1>Features</h1>
  <p>
    Vix File Edit View Go Run AI DB Git Org Tools Help — everything below is
    reachable from the menu bar, the command palette (<code>Ctrl+P</code>),
    or a keybinding. See the
    <a href="https://github.com/vixide/vix/blob/main/index.md">full README</a>
    on GitHub for the complete, exact wording.
  </p>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Core</p>
    <h2>Editing</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <ul>
      <li><strong>Editor</strong> — tabs, undo/redo, tree-sitter syntax highlighting, soft wrap, bracket matching, absolute or relative line numbers, visible whitespace, indent guides, rainbow brackets, sticky scroll, a code-overview minimap, a read-only lock, and configurable indentation.</li>
      <li><strong>Editing comforts</strong> — Smart Home, comment toggle and comment banners, surround selection with brackets/quotes, align lines on a delimiter, increment/decrement and toggle the value under the cursor, delete or transpose a character/word/sentence/paragraph/section, wrap (fill) to a column, Emmet expansion, find next/previous occurrence of the selection, live go-to-line preview, and on-save trim-trailing-whitespace / ensure-final-newline / format-on-save / auto-save / auto-reload. On macOS, <code>Command</code> drives the same bindings as <code>Control</code>.</li>
      <li><strong>Line &amp; text transforms</strong> — sort, dedupe, shuffle, reverse, squeeze blank lines, convert line endings (LF/CRLF), case conversions, and ROT13.</li>
      <li><strong>Clipboard history</strong> — a kill-ring of recent copies/cuts with a paste-from-history picker.</li>
      <li><strong>Persistent undo</strong> — the branch-preserving undo tree is saved per file and restored on reopen.</li>
      <li><strong>Multi-cursor &amp; column editing</strong> — a caret on every match of the selection, or a rectangular block.</li>
      <li><strong>Edit surfaces</strong> — view and edit the active buffer as a CSV/TSV table, a folding prose outline, a JSON/YAML tree, a hex byte dump, or a SQL statement list.</li>
      <li><strong>Snippets</strong> — a searchable picker plus prefix-and-Tab expansion, loaded from bundled, global, per-media-type, and project scopes.</li>
    </ul>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Finding things</p>
    <h2>Navigation &amp; search</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <ul>
      <li><strong>Command palette</strong> (<code>Ctrl+P</code>) — five modes via prefix: file finder, <code>&gt;</code> commands, <code>#</code> buffers, <code>:</code> go-to-line, <code>@</code> go-to-symbol.</li>
      <li><strong>Find &amp; Replace</strong> — incremental search, Case / Smart-case / Whole-Word / Regex toggles, capture groups; workspace-wide search/replace and interactive query-replace.</li>
      <li><strong>Go to definition</strong> (<code>F12</code>) — heuristic, language-agnostic jump to a symbol's likely definition.</li>
      <li><strong>Navigation</strong> — jump to line by label, matching tag (HTML/XML), go to percent/byte, and structural selection that expands to the enclosing syntax node.</li>
      <li><strong>Highlight word occurrences</strong>, <strong>which-key</strong>, and <strong>position history</strong> (back/forward through cursor positions, like a browser).</li>
    </ul>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Working set</p>
    <h2>Files &amp; project</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <ul>
      <li><strong>File explorer</strong> — a directory tree with ephemeral preview tabs, copy/cut/paste with conflict prompts, multi-select, and delete; open buffers follow file moves.</li>
      <li><strong>Project menu</strong> — per-project-type lifecycle commands (configure, compile, test, install, package, run), tasks discovered from npm/yarn/pnpm/bun, Deno, Composer, just, go-task, Rake, and Make, test-at-point, and a monorepo subproject family.</li>
      <li><strong>Integrated terminal &amp; tasks</strong> — a shell in a panel, plus named <code>tasks.toml</code> commands and a compare-with-file diff.</li>
      <li><strong>Split panes</strong> — nested horizontal/vertical splits up to a 2×2 grid.</li>
      <li><strong>Scratch buffer</strong> — a throwaway, unsaved buffer for quick notes.</li>
    </ul>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Code intelligence</p>
    <h2>Git, LSP &amp; debugging</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <ul>
      <li><strong>Git</strong> — status/diff/blame, stage/unstage/revert per hunk, diff navigation, branch switch &amp; merge, stash, amend, and a merge-conflict resolver.</li>
      <li><strong>Language Server Protocol</strong> — diagnostics, hover, completion, go-to, references, call hierarchy, rename, code actions/lens, and inlay hints, configured per language.</li>
      <li><strong>Debugger</strong> — a DAP client: breakpoints, stepping, call stack, variables, watches, and an evaluate REPL.</li>
      <li><strong>Test runner</strong> — parses <code>cargo test</code>/pytest-style output into a pass/fail panel with jump-to-failure.</li>
      <li><strong>TODO finder</strong> — scan the project for TODO/FIXME/HACK/XXX/BUG/NOTE comment tags into a jump-list.</li>
    </ul>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Data</p>
    <h2>Database workbench</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <p>
      Connect to SQLite, PostgreSQL, or MySQL over embedded drivers (no client
      tools needed) and browse a schema tree, run queries in a
      syntax-highlighted editor with autocomplete, and read results in a
      filterable grid. Read-only by default; async execution with
      <code>Ctrl+C</code> cancel and streamed large results; transactions with
      a TX badge; a natural language → SQL AI assistant (schema-only,
      EXPLAIN-validated); query log, a Mermaid ER diagram, CSV/TSV import,
      <code>:name</code> parameters, staged cell edits, an ASCII result
      chart, SSH tunnels, and an OS-keyring credential waterfall.
    </p>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Notes &amp; outlines</p>
    <h2>Org mode</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <p>
      Headline promote/demote, subtree move, TODO cycling, checkbox toggle,
      fold cycling, export to Markdown/HTML, a structural table editor with
      <code>TBLFM</code> formulas, an interactive Column View overlay, and
      Org-roam: nodes, <code>[[</code>-completion, a dailies calendar, and a
      live backlinks panel.
    </p>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Look &amp; feel</p>
    <h2>Themes, keymaps &amp; languages</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <ul>
      <li><strong>Themes</strong> — Dark, Light, and more ship bundled, plus you can add your own.</li>
      <li><strong>Keymaps</strong> — switch the keyboard bindings among Apple, VS Code (macOS/Windows), Emacs, Vi, Spacemacs, IntelliJ (macOS/Windows), Eclipse, and Sublime Text.</li>
      <li><strong>Internationalization</strong> — the whole UI is translatable into many languages.</li>
      <li><strong>Focus &amp; navigation</strong> — Zen mode hides the docks and status bar; an optional breadcrumb bar shows <code>file ▸ symbol</code>.</li>
      <li><strong>Rich status bar</strong> — language, line ending, encoding, selection count, and line:column.</li>
      <li><strong>Mouse support</strong> and <strong>image viewing</strong> (PNG/JPG/GIF, on a graphics-capable terminal).</li>
      <li><strong>Keyboard help</strong> — press <code>F1</code> for an overlay of every shortcut.</li>
    </ul>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Other tools</p>
    <h2>Insert, HTTP, and more</h2>
  </header>
  <div class="prose" style="margin: 0 auto;">
    <ul>
      <li><strong>Insert</strong> — UUID/ZID, Markdown/HTML/SQL/LaTeX/Org snippets, inline Org markers, Lorem ipsum, and Date/Time presets; plus a QR Code generator.</li>
      <li><strong>HTTP client</strong> — send a request from a <code>.http</code> buffer and open the response in a tab.</li>
      <li><strong>Media types</strong> — a searchable MIME catalog with insert and extension lookup.</li>
      <li><strong>Nerd Font palette</strong> — browse a grid of Nerd Font glyphs and insert one into the editor.</li>
      <li><strong>Calendar box</strong> — local date/time, UTC ISO-8601 instant, ISO-8601 week date, and a navigable month grid.</li>
      <li><strong>Message drawer</strong> — advice and notifications, each individually dismissable.</li>
    </ul>
  </div>
</section>

<section class="section prose" style="margin: 0 auto;">
  <p style="text-align: center;">
    <a class="button button-primary" href="/install/">Install Vix →</a>
  </p>
</section>
