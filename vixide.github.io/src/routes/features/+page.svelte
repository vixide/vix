<script lang="ts">
  const docs = 'https://vixide.github.io/vix/';

  const sections = [
    { id: 'editing', label: 'Editing' },
    { id: 'navigation', label: 'Navigation and search' },
    { id: 'files', label: 'Files and projects' },
    { id: 'code', label: 'Code intelligence and version control' },
    { id: 'data', label: 'Data and documents' },
    { id: 'org', label: 'Org mode' },
    { id: 'keys', label: 'Keys, themes and languages' },
    { id: 'trust', label: 'Safety and control' },
    { id: 'tools', label: 'Other tools' },
    { id: 'desktop', label: 'Desktop GUIs' },
    { id: 'demos', label: 'Demos' }
  ];

  // Every recording in the main repo's docs/demos/, copied verbatim.
  const demos = [
    { file: 'overview', title: 'Overview', text: 'Open a file, edit, save, find, and browse the keyboard shortcuts.' },
    { file: 'palette', title: 'Command palette', text: 'Fuzzy file search across the workspace, then command mode.' },
    { file: 'multi-cursor', title: 'Multiple cursors', text: 'Select every occurrence of a word and edit them all at once.' },
    { file: 'git-hunks', title: 'Git hunk staging', text: 'Two changes in one file, staged one hunk at a time.' },
    { file: 'db-workbench', title: 'Database workbench', text: 'Connect to a SQLite database, run a join, view the grid.' },
    { file: 'org-roam', title: 'Org-roam', text: 'Follow a link between notes, check backlinks, view the node graph.' },
    { file: 'edit-surfaces', title: 'Table editor', text: 'Edit a CSV file as an interactive table.' },
    { file: 'themes', title: 'Syntax highlighting', text: 'The default theme across a range of Rust constructs.' }
  ];
</script>

<svelte:head>
  <meta
    name="description"
    content="Everything Vix does: editing, navigation, LSP, a DAP debugger, Git and Jujutsu, a database workbench, Org mode, ten keymaps, themes, scripting, desktop GUIs, and recorded demos."
  />
</svelte:head>

<section class="section prose centered">
  <h1>Features</h1>
  <p>
    Everything below ships in the one <code>vix</code> program and is
    reachable from the menu bar (Vix File Edit View Go Run AI DB JJ Git Org
    Project Tools Help), the command palette (<kbd>Ctrl+P</kbd>), or a key.
    The <a href={docs}>documentation</a> covers each one in detail.
  </p>
  <nav class="toc" aria-label="On this page">
    <ul>
      {#each sections as section (section.id)}
        <li><a href={`#${section.id}`}>{section.label}</a></li>
      {/each}
    </ul>
  </nav>
</section>

<section class="section" id="editing" aria-labelledby="editing-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Core</p>
    <h2 id="editing-heading">Editing</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>Editor</strong> — tabs and split panes, Tree-sitter syntax highlighting, soft wrap, bracket matching, absolute or relative line numbers, visible whitespace, indent guides, rainbow brackets, sticky scroll, a minimap, a read-only lock, and configurable indentation.</li>
      <li><strong>Editing comforts</strong> — Smart Home, comment toggle (<kbd>Ctrl+/</kbd>) and comment banners, surround a selection with brackets or quotes, align lines on a delimiter, increment, decrement or toggle the value under the cursor, delete or transpose a character, word, sentence or paragraph, fill a paragraph to a column, and Emmet expansion.</li>
      <li><strong>On save</strong> — trim trailing whitespace, ensure a final newline, format-on-save, plus auto-save and auto-reload.</li>
      <li><strong>Multiple cursors and column editing</strong> — a caret on every match of the selection, or a rectangular block.</li>
      <li><strong>Line and text transforms</strong> — sort, dedupe, shuffle, reverse, squeeze blank lines, convert line endings, change case, and ROT13.</li>
      <li><strong>Clipboard history</strong> — recent copies and cuts, with a paste-from-history picker.</li>
      <li><strong>Persistent undo</strong> — the branching undo tree is saved per file and restored when you reopen it.</li>
      <li><strong>Snippets and macros</strong> — a searchable snippet picker plus prefix-and-Tab expansion, and recorded keyboard macros.</li>
    </ul>
  </div>
</section>

<section class="section" id="navigation" aria-labelledby="navigation-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Finding things</p>
    <h2 id="navigation-heading">Navigation and search</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>Command palette</strong> (<kbd>Ctrl+P</kbd>) — five modes by prefix: files, <code>&gt;</code> commands, <code>#</code> open buffers, <code>:</code> go to line, <code>@</code> go to symbol.</li>
      <li><strong>Find and replace</strong> — incremental search with case, smart-case, whole-word and regex toggles and capture groups; workspace-wide search and replace, and interactive query-replace.</li>
      <li><strong>Go to definition</strong> (<kbd>F12</kbd>) — through a language server, or a language-agnostic heuristic without one.</li>
      <li><strong>Moving around</strong> — leap-style jump labels, matching HTML/XML tag, go to percent or byte, structural selection that grows to the enclosing syntax node, and back/forward position history (<kbd>Alt+Left</kbd>/<kbd>Alt+Right</kbd>).</li>
      <li><strong>Highlights and hints</strong> — every occurrence of the word under the cursor, and a which-key popup while a chord is pending.</li>
    </ul>
  </div>
</section>

<section class="section" id="files" aria-labelledby="files-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Working set</p>
    <h2 id="files-heading">Files and projects</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>File explorer</strong> — a directory tree with preview tabs, copy/cut/paste with conflict prompts, multi-select, and delete (to the trash by default); open buffers follow moved files.</li>
      <li><strong>Project menu</strong> — configure, compile, test, install, package and run commands per project type; tasks discovered from npm, yarn, pnpm, bun, Deno, Composer, just, go-task, Rake and Make; test-at-point; monorepo subprojects.</li>
      <li><strong>Integrated terminal</strong> — a shell in a panel, plus named commands from <code>tasks.toml</code>.</li>
      <li><strong>Diffs</strong> — compare a buffer with a file, or open <code>vix --diff OLD NEW</code> from the command line.</li>
      <li><strong>Scratch buffer</strong> — a throwaway, unsaved buffer for quick notes.</li>
    </ul>
  </div>
</section>

<section class="section" id="code" aria-labelledby="code-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Code intelligence</p>
    <h2 id="code-heading">Code intelligence and version control</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>Language Server Protocol</strong> — diagnostics, hover, completion, go to definition, references, call hierarchy, rename, code actions, code lens and inlay hints, configured per language.</li>
      <li><strong>Debugger</strong> — a Debug Adapter Protocol client with breakpoints, stepping, call stack, variables, watches and an evaluate REPL.</li>
      <li><strong>Git</strong> — status, diff and blame; stage, unstage or revert one hunk at a time; branch switch and merge, stash, amend, log and file history, and a merge-conflict resolver.</li>
      <li><strong>Jujutsu</strong> — a JJ menu for status, diff, log and operation log, bookmarks, new, describe, commit, squash, abandon, restore, rebase, undo, and <code>jj git push</code>/<code>fetch</code>.</li>
      <li><strong>Test runner</strong> — turns <code>cargo test</code> or pytest-style output into a pass/fail panel with jump-to-failure.</li>
      <li><strong>TODO finder</strong> — collects TODO, FIXME, HACK, XXX, BUG and NOTE comments into a jump list.</li>
    </ul>
  </div>
</section>

<section class="section" id="data" aria-labelledby="data-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Data</p>
    <h2 id="data-heading">Data and documents</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li>
        <strong>Database workbench</strong> — connect to SQLite, PostgreSQL or MySQL
        through embedded drivers (no client tools needed), browse a schema tree,
        run queries with autocomplete, and read results in a filterable grid.
        Read-only by default; cancellable async queries with streamed results;
        transactions; <code>:name</code> parameters; staged cell edits; CSV/TSV
        import; a Mermaid ER diagram; SSH tunnels; credentials from the OS keyring;
        and an optional natural-language-to-SQL assistant that sees only the schema.
      </li>
      <li><strong>Edit surfaces</strong> — view and edit the active buffer as a CSV/TSV table, a folding prose outline, a JSON or YAML tree, a hex byte dump, or a list of SQL statements.</li>
      <li><strong>HTTP client</strong> — send a request from a <code>.http</code> buffer and read the response in a tab.</li>
      <li><strong>Markdown preview</strong> and <strong>image viewing</strong> (PNG, JPEG, GIF and more, on a graphics-capable terminal).</li>
    </ul>
  </div>
</section>

<section class="section" id="org" aria-labelledby="org-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Notes and outlines</p>
    <h2 id="org-heading">Org mode</h2>
  </header>
  <div class="prose centered">
    <p>
      Headline promote and demote, subtree moves, TODO cycling, checkboxes,
      folding, capture, an agenda, export to Markdown and HTML, a structural
      table editor with <code>TBLFM</code> formulas, an interactive Column View,
      contacts with vCard export, and Org-roam: linked notes with link
      completion, a dailies calendar, and a live backlinks panel.
    </p>
  </div>
</section>

<section class="section" id="keys" aria-labelledby="keys-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Look and feel</p>
    <h2 id="keys-heading">Keys, themes and languages</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>Ten keymaps</strong> — Apple (the default), VS Code for macOS and Windows, Emacs, Vi, Spacemacs, IntelliJ for macOS and Windows, Eclipse, and Sublime Text, switchable live from <strong>View → Keymap</strong>. On macOS, <kbd>Command</kbd> works wherever <kbd>Control</kbd> does.</li>
      <li><strong>Discoverable</strong> — a menu bar for every action, a searchable overlay of every shortcut (<kbd>F1</kbd>), and full mouse support.</li>
      <li><strong>Themes</strong> — 20 bundled, including high-contrast and Catppuccin; <code>theme = "auto"</code> follows your terminal's light or dark background; a theme editor; and your own JSON themes.</li>
      <li><strong>Languages</strong> — the interface is translated into 15 languages, including Arabic, Bengali, Chinese, Hindi, Japanese, Russian, Welsh, Irish and Scottish Gaelic.</li>
      <li><strong>Focus</strong> — Zen mode hides the docks and status bar; a breadcrumb bar shows <code>file ▸ symbol</code>; the status bar shows language, line ending, encoding, selection size and position.</li>
    </ul>
  </div>
</section>

<section class="section" id="trust" aria-labelledby="trust-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Careful by default</p>
    <h2 id="trust-heading">Safety and control</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>No <code>unsafe</code> Rust</strong> — forbidden in every crate of the workspace.</li>
      <li><strong>Scripting with limits</strong> — <a href={`${docs}scripting/index.html`}>Rhai scripts</a> add commands, key bindings and buffer transforms without a rebuild, but get no file, network or process access, and a project's own scripts run only after you trust that workspace.</li>
      <li><strong>Read-only databases</strong> — the workbench opens connections read-only until you allow writes.</li>
      <li><strong>Minimal build</strong> — a release build with no network, shell, scripting or external-program features at all; see <a href="/install/#minimal">Install</a>.</li>
    </ul>
  </div>
</section>

<section class="section" id="tools" aria-labelledby="tools-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Extras</p>
    <h2 id="tools-heading">Other tools</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li><strong>AI menu</strong> — runs the assistant command-line tool you configure (Claude, Codex, a local model, …) on the selection or file: summarize, explain or define in a new tab, or annotate and improve it in place (undo reverts it).</li>
      <li><strong>Insert</strong> — UUIDs, Markdown/HTML/SQL/LaTeX/Org snippets, Lorem ipsum, date and time presets, and QR codes.</li>
      <li><strong>Pickers</strong> — Nerd Font glyphs, media types, and more from the Tools menu.</li>
      <li><strong>Calendar</strong> — local time, UTC instant, ISO week date, and a month grid.</li>
      <li><strong>Spell checking</strong> with Hunspell dictionaries.</li>
    </ul>
  </div>
</section>

<section class="section" id="desktop" aria-labelledby="desktop-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Beyond the terminal</p>
    <h2 id="desktop-heading">Desktop GUIs</h2>
  </header>
  <div class="prose centered">
    <p>
      The whole of Vix also runs in a native window on macOS, Windows and Linux.
      Five experimental front ends — <code>vix-gpui</code>, <code>vix-egui</code>,
      <code>vix-iced</code>, <code>vix-slint</code>, and source-only
      <code>vix-gtk4</code> — are each built on a different toolkit so they can be
      compared. See <a href="/install/#desktop">Install</a>.
    </p>
  </div>
</section>

<section class="section" id="demos" aria-labelledby="demos-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Recorded in a real terminal</p>
    <h2 id="demos-heading">Demos</h2>
  </header>
  <div class="demo-grid">
    {#each demos as demo (demo.file)}
      <figure class="demo">
        <img
          src={`/assets/demos/${demo.file}.gif`}
          width="1171"
          height="784"
          loading="lazy"
          alt={`Vix demo, ${demo.title}: ${demo.text}`}
        />
        <figcaption><strong>{demo.title}.</strong> {demo.text}</figcaption>
      </figure>
    {/each}
  </div>
  <p class="section-more">
    <a class="button button-primary" href="/install/">Install Vix</a>
  </p>
</section>
