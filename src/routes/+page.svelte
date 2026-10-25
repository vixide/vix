<script lang="ts">
  import { Card } from '@lilydesignsystem/svelte-headless';
  import CopyCommand from '#lib/CopyCommand.svelte';

  const docs = 'https://vixide.github.io/vix/';

  // Short demo recordings, rendered from the main repo's docs/demos/*.tape
  // VHS scripts against its demo workspace and copied here verbatim.
  const demos = [
    { file: 'palette', title: 'Command palette', text: 'Fuzzy-find files, then switch to command mode with >.' },
    { file: 'multi-cursor', title: 'Multiple cursors', text: 'Select every occurrence of a word and edit them all at once.' },
    { file: 'git-hunks', title: 'Git hunk staging', text: 'Stage two changes in one file, one hunk at a time.' },
    { file: 'db-workbench', title: 'Database workbench', text: 'Connect to a SQLite file, run a join, read the results grid.' }
  ];

  // The docs' for-*-users guides, in the main repo's docs/.
  const comingFrom = [
    { slug: 'for-vim-users', name: 'Vim' },
    { slug: 'for-emacs-users', name: 'Emacs' },
    { slug: 'for-visual-studio-code-users', name: 'VS Code' },
    { slug: 'for-helix-users', name: 'Helix' },
    { slug: 'for-spacemacs-users', name: 'Spacemacs' },
    { slug: 'for-intellij-users', name: 'IntelliJ' },
    { slug: 'for-eclipse-users', name: 'Eclipse' },
    { slug: 'for-sublime-users', name: 'Sublime Text' }
  ];
</script>

<svelte:head>
  <meta
    name="description"
    content="Vix is a terminal text editor and IDE written in Rust, with familiar Ctrl/Cmd keys and a menu bar, plus built-in LSP, a debugger, Git and Jujutsu, Org mode, a database workbench, and an integrated terminal."
  />
</svelte:head>

<section class="site-hero" aria-labelledby="hero-heading">
  <p class="section-heading-eyebrow">Terminal text editor and IDE · Rust · Free and open source</p>
  <h1 id="hero-heading">A terminal IDE that uses the keys you already know.</h1>
  <p class="site-hero-tagline">
    Save is <kbd>Ctrl+S</kbd>, find is <kbd>Ctrl+F</kbd>, and everything else is
    in a menu bar or the <kbd>Ctrl+P</kbd> command palette — no modes to learn
    first. Language servers, a debugger, Git, Org mode and a database workbench
    are built in, so Vix is useful before you configure anything.
  </p>
  <div class="site-hero-install">
    <CopyCommand command="brew install vixide/homebrew-tap/vix-tui" label="Copy the Homebrew install command" />
    <p class="site-hero-install-note">
      Or use the installer script, PowerShell, npm, or <code>cargo install vix-tui</code> —
      see <a href="/install/">all install options</a>. The command is <code>vix</code>.
    </p>
  </div>
  <div class="button-row">
    <a class="button button-primary" href="/install/">Install Vix</a>
    <a class="button button-secondary" href="/features/">See all features</a>
    <a class="button button-secondary" href="https://github.com/vixide/vix">GitHub</a>
  </div>
</section>

<figure class="screenshot">
  <img
    src="/assets/demos/overview.gif"
    width="1171"
    height="784"
    alt="Vix in a terminal: a menu bar across the top (Vix, File, Edit, View, Go, Run, AI, DB, JJ, Git, Org, Project, Tools, Help), a file explorer on the left, an editor tab in the middle, a messages panel on the right, an output panel below, and a status bar with the Git branch, encoding and cursor position."
  />
  <figcaption>Opening, editing, saving and searching in a demo workspace.</figcaption>
</figure>

<section class="section" aria-labelledby="why-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Why Vix</p>
    <h2 id="why-heading">An IDE in your terminal, without the assembly</h2>
  </header>

  <div class="card-grid">
    <Card heading="Familiar keys, no modes" href="/features/#keys">
      <p class="card-description">
        Ctrl/Cmd shortcuts, a real menu bar, mouse support and <kbd>F1</kbd>
        for every shortcut. Ten keymaps — Apple, VS Code, Emacs, Vi,
        Spacemacs, IntelliJ, Eclipse, Sublime Text — switchable live.
      </p>
    </Card>
    <Card heading="Code intelligence built in" href="/features/#code">
      <p class="card-description">
        Language servers, a DAP debugger, Tree-sitter highlighting, a test
        runner, project tasks and an integrated terminal, in one binary.
      </p>
    </Card>
    <Card heading="Git and Jujutsu" href="/features/#code">
      <p class="card-description">
        Stage, unstage or revert one hunk at a time, blame, stash, branches
        and a merge-conflict resolver — plus a full Jujutsu menu.
      </p>
    </Card>
    <Card heading="Beyond code" href="/features/#data">
      <p class="card-description">
        An Org-mode suite with Org-roam, a SQLite/PostgreSQL/MySQL workbench,
        an HTTP client, and table, JSON/YAML and hex editing views.
      </p>
    </Card>
    <Card heading="Careful by default" href="/features/#trust">
      <p class="card-description">
        No <code>unsafe</code> Rust. Scripts get no file, network or process
        access. The database workbench opens read-only. A minimal build leaves
        out every network and shell feature.
      </p>
    </Card>
    <Card heading="Terminal or desktop" href="#desktop">
      <p class="card-description">
        The same editor runs in a native window on macOS, Windows and Linux
        through experimental desktop front ends.
      </p>
    </Card>
  </div>
</section>

<section class="section" aria-labelledby="demos-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">See it work</p>
    <h2 id="demos-heading">Recorded in a real terminal</h2>
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
  <p class="section-more"><a href="/features/#demos">More demos on the features page</a></p>
</section>

<section class="section" aria-labelledby="start-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Your first minute</p>
    <h2 id="start-heading">The keys to start with</h2>
  </header>
  <div class="prose centered">
    <table class="key-table">
      <thead>
        <tr><th scope="col">Keys</th><th scope="col">Does</th></tr>
      </thead>
      <tbody>
        <tr><td><kbd>Ctrl+P</kbd></td><td>Command palette: find files; type <code>&gt;</code> for commands, <code>@</code> for symbols, <code>:</code> for a line</td></tr>
        <tr><td><kbd>Ctrl+B</kbd></td><td>Show or hide the file explorer</td></tr>
        <tr><td><kbd>Ctrl+O</kbd> · <kbd>Ctrl+S</kbd></td><td>Open · save</td></tr>
        <tr><td><kbd>Ctrl+F</kbd> · <kbd>Ctrl+Shift+F</kbd></td><td>Find in the file · search the workspace</td></tr>
        <tr><td><kbd>Ctrl+R</kbd></td><td>Find and replace</td></tr>
        <tr><td><kbd>Ctrl+Z</kbd> · <kbd>Ctrl+Shift+Z</kbd></td><td>Undo · redo</td></tr>
        <tr><td><kbd>F1</kbd></td><td>Every shortcut in your keymap, searchable</td></tr>
        <tr><td><kbd>Ctrl+Q</kbd></td><td>Quit (Vix asks before discarding unsaved work)</td></tr>
      </tbody>
    </table>
    <p class="small-print">
      These are the default Apple keymap's bindings; on macOS <kbd>Command</kbd>
      works wherever <kbd>Control</kbd> does. See
      <a href={`${docs}getting-started/index.html`}>Getting Started</a>.
    </p>
  </div>
</section>

<section class="section" aria-labelledby="from-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Switching editors?</p>
    <h2 id="from-heading">Read the guide for where you're coming from</h2>
  </header>
  <ul class="link-grid">
    {#each comingFrom as editor (editor.slug)}
      <li><a href={`${docs}${editor.slug}/index.html`}>Vix for {editor.name} users</a></li>
    {/each}
  </ul>
  <p class="section-more">
    Or compare Vix with Vim, Helix, Micro and Zed in the
    <a href={`${docs}comparison/index.html`}>feature comparison</a>.
  </p>
</section>

<section class="section" id="desktop" aria-labelledby="desktop-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Desktop</p>
    <h2 id="desktop-heading">The same editor in a native window</h2>
  </header>
  <div class="prose centered">
    <p>
      Every panel, overlay and dock of Vix also runs in a desktop window on
      macOS, Windows and Linux. Releases ship four front ends, each a
      complete implementation on a different toolkit so they can be compared:
      <code>vix-gpui</code> (GPUI), <code>vix-egui</code> (egui),
      <code>vix-iced</code> (iced) and <code>vix-slint</code> (Slint). A fifth,
      <code>vix-gtk4</code> (GTK 4), builds from source. They are
      <strong>experimental</strong>.
    </p>
    <CopyCommand command="brew install vixide/homebrew-tap/vix-gpui" label="Copy the vix-gpui Homebrew install command" />
  </div>
</section>

<section class="section" aria-labelledby="open-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Open source</p>
    <h2 id="open-heading">Free, and built in the open</h2>
  </header>
  <div class="prose centered">
    <p>
      Vix's source is licensed under your choice of Apache-2.0, BSD-3-Clause,
      MIT, GPL-2.0 or GPL-3.0. Development is specification-driven: each of
      its crates has a spec that the code implements. How the project uses AI
      assistance is set out in its
      <a href="https://github.com/vixide/vix/blob/main/AI_STATEMENT.md">AI statement</a>.
    </p>
    <div class="button-row">
      <a class="button button-secondary" href="https://github.com/vixide/vix">Source on GitHub</a>
      <a class="button button-secondary" href={docs}>Documentation</a>
      <a class="button button-secondary" href="https://github.com/vixide/vix/blob/main/CHANGELOG.md">Changelog</a>
    </div>
  </div>
</section>
