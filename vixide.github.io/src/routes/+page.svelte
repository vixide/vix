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

  // At-a-glance facts. Every number is stated in the main repo (README.md,
  // tui/themes/, crates/vix-i18n/locales/, docs/keymaps/).
  const glance = [
    { value: '1', label: 'binary, nothing to assemble' },
    { value: '10', label: 'keymaps, switchable live' },
    { value: '21', label: 'bundled themes' },
    { value: '15', label: 'interface languages' },
    { value: '0', label: 'lines of unsafe Rust' }
  ];

  // Highlights of the unreleased 2.0, from the main repo's CHANGELOG.md
  // ([Unreleased]); the full list is on the features page.
  const whatsNew = [
    { title: 'Remote editing over SSH', text: 'work on a folder on another machine from File → Open Remote… or vix ssh://host/path, with nothing installed there (experimental).' },
    { title: 'Cowork', text: 'edit the same files together in real time, encrypted, with no server in between (an opt-in build feature).' },
    { title: 'IM menu', text: 'code-aware chat and comment threads anchored to lines of your files.' },
    { title: 'IDE features', text: 'Local History, Search Everywhere, a Refactor menu, run configurations, Peek Definition and CodeLens.' },
    { title: 'Large File Viewer', text: 'a 4 GiB log opens to its first screen in under a millisecond, in under 5 MiB of memory (Apple M4 Max, warm cache).' },
    { title: 'Containers and more', text: 'an OC menu for Docker, Podman and nerdctl, an MCP server mode, and a switch that turns AI off.' }
  ];

  const audiences = [
    {
      heading: 'You live in the terminal but miss your IDE',
      text: 'Go to definition, rename, a debugger and a Git panel, over SSH or in tmux, without a config project.'
    },
    {
      heading: 'You tried Vim or Helix and bounced off modes',
      text: 'Vix starts with the shortcuts every other app uses. Switch on the Vi keymap when you want it.'
    },
    {
      heading: 'You came to Emacs for Org mode',
      text: 'Agenda, tables with formulas, Column View, Org-roam and dailies are built in, with menus to find them.'
    },
    {
      heading: 'You work with data and APIs',
      text: 'Query a database, send an HTTP request, inspect a container and edit the CSV it produced, in one window.'
    }
  ];

  // A short slice of docs/comparison/index.md; the full matrix, with
  // footnotes, lives there.
  const compareEditors = ['Vix', 'Vim', 'Helix', 'Micro', 'Zed'];
  const compareRows: { feature: string; marks: string[] }[] = [
    { feature: 'Runs in a terminal', marks: ['✓', '✓', '✓', '✓', '✗'] },
    { feature: 'Familiar keys by default (no modes)', marks: ['✓', '✗', '✗', '✓', '✓'] },
    { feature: 'LSP, debugger and Tree-sitter built in', marks: ['✓', '✗', '✓', '✗', '✓'] },
    { feature: 'Git client built in', marks: ['✓', '✗', 'partial', '✗', '✓'] },
    { feature: 'Integrated terminal', marks: ['✓', '✓', '✗', '✗', '✓'] },
    { feature: 'Org mode', marks: ['✓', '✗', '✗', '✗', '✗'] }
  ];

  const faq = [
    {
      q: 'Do I have to learn modal editing?',
      a: 'No. The default keymap uses the same shortcuts as your other apps. Vi and Spacemacs keymaps are one menu choice away if you want them.'
    },
    {
      q: 'Do I need plugins to get started?',
      a: 'No. Everything on this page ships in the one vix binary. Rhai scripts are there for adding your own commands, not for filling gaps.'
    },
    {
      q: 'Does it work over SSH and in tmux?',
      a: 'Yes. Vix is a terminal program, so run it on the remote machine as you would any other. Vix 2.0, not released yet, also opens a folder on another machine from your local Vix over SSH (vix ssh://host/path, or File → Open Remote…). That is experimental.'
    },
    {
      q: 'Can my company use it?',
      a: 'Pick whichever of five licenses suits you: Apache-2.0, BSD-3-Clause, MIT, GPL-2.0 or GPL-3.0. For locked-down machines, the minimal build makes no network connections and starts no other program.'
    }
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

<section class="site-hero" aria-labelledby="hero-heading">
  <p class="section-heading-eyebrow">Terminal text editor and IDE · Rust · Free and open source</p>
  <h1 id="hero-heading">The terminal IDE that already knows your keys.</h1>
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

<section class="section" aria-labelledby="glance-heading">
  <h2 id="glance-heading" class="visually-hidden">Vix at a glance</h2>
  <ul class="stat-strip">
    {#each glance as item (item.label)}
      <li><span class="stat-value">{item.value}</span> <span class="stat-label">{item.label}</span></li>
    {/each}
  </ul>
</section>

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
        Stage, unstage or revert one hunk at a time, blame, stash, branches,
        an interactive commit graph and a merge-conflict resolver — plus a
        full Jujutsu menu.
      </p>
    </Card>
    <Card heading="Beyond code" href="/features/#data">
      <p class="card-description">
        An Org-mode suite with Org-roam, a SQLite/PostgreSQL/MySQL workbench,
        a REST client for <code>.http</code> files, a container menu for
        Docker and Podman, and table, JSON/YAML and hex editing views.
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

<section class="section" aria-labelledby="new-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Coming in 2.0</p>
    <h2 id="new-heading">What's new in the next release</h2>
  </header>
  <div class="prose centered">
    <ul>
      {#each whatsNew as item (item.title)}
        <li><strong>{item.title}</strong> — {item.text}</li>
      {/each}
    </ul>
    <p class="small-print">
      2.0 is not released yet; the current release is 1.8.
      <a href="/features/#new">Everything new in 2.0</a>.
    </p>
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

<section class="section" aria-labelledby="who-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Who it's for</p>
    <h2 id="who-heading">Made for the way you already work</h2>
  </header>
  <div class="card-grid">
    {#each audiences as audience (audience.heading)}
      <Card heading={audience.heading}>
        <p class="card-description">{audience.text}</p>
      </Card>
    {/each}
  </div>
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
</section>

<section class="section" aria-labelledby="compare-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">How it compares</p>
    <h2 id="compare-heading">Built in, not bolted on</h2>
  </header>
  <div class="prose centered table-scroll">
    <table class="compare-table">
      <thead>
        <tr>
          <th scope="col">Feature</th>
          {#each compareEditors as editor (editor)}<th scope="col">{editor}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each compareRows as row (row.feature)}
          <tr>
            <th scope="row">{row.feature}</th>
            {#each row.marks as mark, i (i)}<td>{mark}</td>{/each}
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="small-print">
      Built in means no plugin needed. The
      <a href={`${docs}comparison/index.html`}>full comparison</a> has footnotes
      and the places where Vix is behind.
    </p>
  </div>
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

<section class="section" aria-labelledby="faq-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Questions</p>
    <h2 id="faq-heading">Frequently asked</h2>
  </header>
  <div class="prose centered faq">
    {#each faq as item (item.q)}
      <details>
        <summary>{item.q}</summary>
        <p>{item.a}</p>
      </details>
    {/each}
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

<section class="section cta" aria-labelledby="cta-heading">
  <h2 id="cta-heading">Try Vix in thirty seconds</h2>
  <div class="prose centered">
    <CopyCommand command="brew install vixide/homebrew-tap/vix-tui" label="Copy the Homebrew install command" />
    <p class="small-print">Then run <code>vix</code> in any folder. Press <kbd>F1</kbd> for every shortcut.</p>
  </div>
  <div class="button-row">
    <a class="button button-primary" href="/install/">All install options</a>
    <a class="button button-secondary" href="https://github.com/vixide/vix">Star on GitHub</a>
  </div>
</section>
