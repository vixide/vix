<script lang="ts">
  import { Card } from '@lilydesignsystem/svelte-headless';
  import CopyCommand from '#lib/CopyCommand.svelte';
  import Rich from '#lib/Rich.svelte';
  import { t } from '#lib/i18n/index.svelte.ts';

  const docs = 'https://vixide.github.io/vix/';

  // Short demo recordings, rendered from the main repo's docs/demos/*.tape
  // VHS scripts against its demo workspace and copied here verbatim.
  const demos = [
    { file: 'palette', title: 'Command palette', text: 'Fuzzy-find files, then switch to command mode with >.' },
    { file: 'multi-cursor', title: 'Multiple cursors', text: 'Select every occurrence of a word and edit them all at once.' },
    { file: 'git-hunks', title: 'Git hunk staging', text: 'Stage two changes in one file, one hunk at a time.' },
    { file: 'db-workbench', title: 'Database workbench', text: 'Connect to a SQLite file, run a join, read the results grid.' }
  ];


  // The eight front ends, vix-tui first, then alphabetically: the same
  // two sentences as the main repo's README ("TUI & GUI capabilities").
  // `brew` is whether vixide/homebrew-tap has a formula for it.
  const frontends = [
    { name: 'vix-tui', brew: true, what: 'The terminal editor, the vix command, and the App every other front end wraps.', why: 'Start here for the most complete and most tested Vix: it runs in any terminal on macOS, Windows and Linux, over SSH and inside tmux.' },
    { name: 'vix-bui', brew: false, what: 'Vix in a native window drawn by BUI, Blender’s user-interface toolkit used as a library.', why: 'Try it if you like Blender’s dense, flat look; it is GPL-3.0-only and needs CMake to build.' },
    { name: 'vix-egui', brew: true, what: 'Vix in a native window drawn by egui through eframe, an immediate-mode GUI.', why: 'Try it for the simplest GPU front end, with input-method typing and a native macOS menu bar.' },
    { name: 'vix-gpui', brew: true, what: 'Vix in a native window on GPUI, the GPU framework behind the Zed editor.', why: 'Try it for the operating system’s own text rendering and a native menu bar on macOS, Windows and Linux.' },
    { name: 'vix-gtk4', brew: false, what: 'Vix in a GTK 4 window, with text drawn by Pango and Cairo.', why: 'Try it on a GNOME or other GTK desktop, where it fits in natively with full input-method support; it needs GTK 4 installed.' },
    { name: 'vix-iced', brew: true, what: 'Vix in a native window on iced, an Elm-style Rust GUI library.', why: 'Try it for advanced text shaping and per-glyph font fallback that handle mixed scripts well.' },
    { name: 'vix-slint', brew: true, what: 'Vix in a native window on Slint, a declarative UI toolkit drawn with OpenGL.', why: 'Try it for native menu bars on macOS and Windows; it is GPL-3.0-only.' },
    { name: 'vix-zgui', brew: true, what: 'Vix in a native window on zgui, a reactive Rust GUI with CSS styling and a wgpu renderer.', why: 'Try it for the newest front end, with text shaped by parley; it is a first version.' }
  ];

  // Highlights of 2.0, from the main repo's CHANGELOG.md ([2.0.0]); the
  // full list is on the features page.
  const whatsNew = [
    { title: 'Remote editing over SSH', text: 'work on a folder on another machine from File → Open Remote… or vix ssh://host/path, with nothing installed there (experimental).' },
    { title: 'Cowork', text: 'edit the same files together in real time, encrypted, with no server in between (an opt-in build feature).' },
    { title: 'IM menu', text: 'code-aware chat and comment threads anchored to lines of your files.' },
    { title: 'IDE features', text: 'Local History, Search Everywhere, a Refactor menu, run configurations, Peek Definition and CodeLens.' },
    { title: 'Large File Viewer', text: 'a 4 GiB log opens to its first screen in under a millisecond, in under 5 MiB of memory (Apple M4 Max, warm cache).' },
    { title: 'Containers and more', text: 'an OC menu for Docker, Podman and nerdctl, an MCP server mode, and a switch that turns AI off.' }
  ];

  // Their copy is in the message catalog (who.N.heading / who.N.text).
  const audiences = [1, 2, 3, 4] as const;

  // Built into a stock install, with no plugin or extension. `✓` built
  // in, `-` not built in (a plugin or extension may exist), `partial`
  // limited (see the small print). The docs' comparison page has the
  // footnoted version for the first columns, Fresh and ttt included.
  const compareEditors = ['Vix', 'Vim', 'Emacs', 'Neovim', 'Helix', 'VS Code', 'Zed', 'Fresh', 'ttt'];
  const compareRows: { feature: string; marks: string[] }[] = [
    { feature: 'Runs in a terminal', marks: ['✓', '✓', '✓', '✓', '✓', '-', '-', '✓', '✓'] },
    { feature: 'Native desktop window', marks: ['✓', '✓', '✓', '-', '-', '✓', '✓', '✓', '-'] },
    { feature: 'Common shortcuts by default (Ctrl+S, Ctrl+C, Ctrl+V)', marks: ['✓', '-', '-', '-', '-', '✓', '✓', '✓', '✓'] },
    { feature: 'Menu bar', marks: ['✓', '-', '✓', '-', '-', '✓', '✓', '✓', '✓'] },
    { feature: 'Command palette and fuzzy file finder', marks: ['✓', '-', 'partial', '-', '✓', '✓', '✓', '✓', '✓'] },
    { feature: 'Multiple cursors', marks: ['✓', '-', '-', '-', '✓', '✓', '✓', '✓', '✓'] },
    { feature: 'Language servers (LSP)', marks: ['✓', '-', '✓', '✓', '✓', 'partial', '✓', '✓', '✓'] },
    { feature: 'Debugger (DAP)', marks: ['✓', '-', '-', '-', '✓', '✓', '✓', '-', '-'] },
    { feature: 'Tree-sitter highlighting', marks: ['✓', '-', 'partial', '✓', '✓', '-', '✓', '✓', '-'] },
    { feature: 'Integrated terminal', marks: ['✓', '✓', '✓', '✓', '-', '✓', '✓', '✓', '✓'] },
    { feature: 'Git client', marks: ['✓', '-', 'partial', '-', 'partial', '✓', '✓', 'partial', '✓'] },
    { feature: 'Jujutsu client', marks: ['✓', '-', '-', '-', '-', '-', '-', '-', '-'] },
    { feature: 'Remote editing over SSH', marks: ['✓', '✓', '✓', '✓', '-', '-', '✓', 'partial', '-'] },
    { feature: 'Database workbench', marks: ['✓', '-', 'partial', '-', '-', '-', '-', '-', '-'] },
    { feature: 'HTTP client for .http files', marks: ['✓', '-', '-', '-', '-', '-', '-', '-', '-'] },
    { feature: 'Org mode', marks: ['✓', '-', '✓', '-', '-', '-', '-', '-', '-'] },
    { feature: 'Themes built in', marks: ['✓', '✓', '✓', '✓', '✓', '✓', '✓', '✓', '✓'] },
    { feature: 'Interface in many languages', marks: ['✓', '✓', '-', '✓', '-', '-', '-', '✓', '-'] }
  ];

  // Their copy is in the message catalog (faq.N.q / faq.N.a).
  const faq = [1, 2, 3, 4] as const;

  // The docs' for-*-users guides, in the main repo's docs/.
  const comingFrom = [
    { slug: 'for-vim-users', name: 'Vim' },
    { slug: 'for-emacs-users', name: 'Emacs' },
    { slug: 'for-visual-studio-code-users', name: 'VS Code' },
    { slug: 'for-helix-users', name: 'Helix' },
    { slug: 'for-spacemacs-users', name: 'Spacemacs' },
    { slug: 'for-intellij-users', name: 'IntelliJ' },
    { slug: 'for-eclipse-users', name: 'Eclipse' },
    { slug: 'for-sublime-users', name: 'Sublime' },
    { slug: 'for-zed-users', name: 'Zed' }
  ];
</script>

<section class="site-hero" aria-labelledby="hero-heading">
  <h1 id="hero-heading">{t('hero.title')}</h1>
  <p class="site-hero-tagline"><Rich text={t('hero.tagline')} /></p>
  <div class="site-hero-install">
    <CopyCommand command="brew install vixide/homebrew-tap/vix-tui" label={t('hero.copyBrew')} />
    <p class="site-hero-install-note"><Rich text={t('hero.installNote')} href="/install/" /></p>
  </div>
  <div class="button-row">
    <a class="button button-primary" href="/install/">{t('hero.install')}</a>
    <a class="button button-secondary" href="/features/">{t('hero.features')}</a>
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
    <h2 id="why-heading">Why developers pick Vix</h2>
  </header>

  <div class="card-grid">
    <Card heading="Productive in the first minute" href="/features/#keys">
      <p class="card-description">
        No modes, no config project. <kbd>Ctrl+S</kbd> saves, a real menu bar
        shows what exists, and <kbd>F1</kbd> lists every shortcut. Prefer Vim,
        Emacs, VS Code or IntelliJ keys? Fourteen keymaps switch live.
      </p>
    </Card>
    <Card heading="An IDE, not a kit to assemble" href="/features/#code">
      <p class="card-description">
        Language servers, a debugger, Tree-sitter highlighting, a test runner,
        tasks and an integrated terminal come in the box, so there are no
        plugins to choose, install or keep working.
      </p>
    </Card>
    <Card heading="Git and Jujutsu without leaving the file" href="/features/#code">
      <p class="card-description">
        Stage or revert one hunk at a time, blame a line, browse the commit
        graph and resolve merge conflicts in place, with a full Jujutsu menu
        beside Git.
      </p>
    </Card>
    <Card heading="One window for code, notes and data" href="/features/#data">
      <p class="card-description">
        Org mode with Org-roam, a SQLite, PostgreSQL and MySQL workbench, an
        HTTP client, a container menu and spreadsheet, JSON and hex views
        replace a handful of separate tools.
      </p>
    </Card>
    <Card heading="Works where you work" href="/features/#keys">
      <p class="card-description">
        Over SSH, inside tmux and in any terminal on macOS, Windows and Linux,
        or in a native desktop window, with the whole interface in 15
        languages.
      </p>
    </Card>
    <Card heading="Safe by construction" href="/features/#trust">
      <p class="card-description">
        No <code>unsafe</code> Rust anywhere, sandboxed scripts, a database
        workbench that opens read-only, and a minimal build with no network
        or shell features at all.
      </p>
    </Card>
  </div>
</section>

<section class="section" aria-labelledby="new-heading">
  <header class="section-heading">
    <h2 id="new-heading">What's new in the latest release</h2>
  </header>
  <div class="prose centered">
    <ul>
      {#each whatsNew as item (item.title)}
        <li><strong>{item.title}</strong> — {item.text}</li>
      {/each}
    </ul>
    <p class="small-print">
      The current release is 2.0.2.
      <a href="/whats-new/">What’s new in 2.0</a>, or
      <a href="/features/#new">the complete list</a>.
    </p>
  </div>
</section>

<section class="section" aria-labelledby="demos-heading">
  <header class="section-heading">
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
    <h2 id="who-heading">{t('who.title')}</h2>
  </header>
  <div class="card-grid">
    {#each audiences as n (n)}
      <Card heading={t(`who.${n}.heading`)}>
        <p class="card-description">{t(`who.${n}.text`)}</p>
      </Card>
    {/each}
  </div>
</section>

<section class="section" aria-labelledby="start-heading">
  <header class="section-heading">
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
    <h2 id="from-heading">{t('from.title')}</h2>
  </header>
  <ul class="link-grid">
    {#each comingFrom as editor (editor.slug)}
      <li><a href={`${docs}${editor.slug}/index.html`}>{t('from.link', { name: editor.name })}</a></li>
    {/each}
  </ul>
</section>

<section class="section" aria-labelledby="compare-heading">
  <header class="section-heading">
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
      ✓ built in, with no plugin or extension. - not built in (a plugin or
      extension may exist). Partial: Emacs’s M-x completion is not a fuzzy
      palette, its Tree-sitter needs grammars installed, its VC commits and
      blames but stages no hunks, and its SQL mode is a console rather than a
      workbench; VS Code bundles language servers for
      only a few web languages; Helix has Git gutters, not a Git panel; Fresh has a Git log, grep and
      file finder but no staging, and its SSH workspaces are part of its
      Orchestrator. Fresh and ttt are marked from their own READMEs and
      manifests, as read on 10 November 2026. Vix’s remote editing and desktop windows
      are experimental. The
      <a href={`${docs}compare/index.html`}>full comparison</a> has
      footnotes and the places where Vix is behind.
    </p>
  </div>
</section>

<section class="section" id="install" aria-labelledby="install-heading">
  <header class="section-heading">
    <h2 id="install-heading">Install</h2>
  </header>
  <div class="prose centered">
    <p>
      Vix is one editor with eight front ends: the terminal app and seven
      native desktop windows, each on a different toolkit. Every one installs
      with <code>cargo install</code> (Rust 1.97 or newer); most also install
      with Homebrew on macOS and Linux. The desktop front ends are
      experimental. <a href="/install/">More install options</a> cover the
      installer scripts, PowerShell, npm and Linux packages.
    </p>
  </div>
  <div class="frontend-list">
    {#each frontends as fe (fe.name)}
      <article class="frontend" aria-labelledby={`fe-${fe.name}`}>
        <h3 id={`fe-${fe.name}`}><a href={`https://crates.io/crates/${fe.name}`}>{fe.name}</a></h3>
        <p>{fe.what} {fe.why}</p>
        {#if fe.brew}
          <CopyCommand command={`brew install vixide/homebrew-tap/${fe.name}`} label={`Copy the ${fe.name} Homebrew install command`} />
        {/if}
        <CopyCommand command={`cargo install --locked ${fe.name}`} label={`Copy the ${fe.name} cargo install command`} />
      </article>
    {/each}
  </div>
</section>

<section class="section" aria-labelledby="faq-heading">
  <header class="section-heading">
    <h2 id="faq-heading">{t('faq.title')}</h2>
  </header>
  <div class="prose centered faq">
    {#each faq as n (n)}
      <details>
        <summary>{t(`faq.${n}.q`)}</summary>
        <p><Rich text={t(`faq.${n}.a`)} /></p>
      </details>
    {/each}
  </div>
</section>

<section class="section" aria-labelledby="open-heading">
  <header class="section-heading">
    <h2 id="open-heading">{t('open.title')}</h2>
  </header>
  <div class="prose centered">
    <p>
      <Rich
        text={t('open.text')}
        href="https://github.com/vixide/vix/blob/main/AI_STATEMENT.md"
      />
    </p>
    <div class="button-row">
      <a class="button button-secondary" href="https://github.com/vixide/vix">{t('open.source')}</a>
      <a class="button button-secondary" href={docs}>{t('open.docs')}</a>
      <a class="button button-secondary" href="https://github.com/vixide/vix/blob/main/CHANGELOG.md">{t('open.changelog')}</a>
    </div>
  </div>
</section>

<section class="section cta" aria-labelledby="cta-heading">
  <h2 id="cta-heading">{t('cta.title')}</h2>
  <div class="prose centered">
    <CopyCommand command="brew install vixide/homebrew-tap/vix-tui" label={t('hero.copyBrew')} />
    <p class="small-print"><Rich text={t('cta.note')} /></p>
  </div>
  <div class="button-row">
    <a class="button button-primary" href="/install/">{t('cta.all')}</a>
    <a class="button button-secondary" href="https://github.com/vixide/vix">{t('cta.star')}</a>
  </div>
</section>
