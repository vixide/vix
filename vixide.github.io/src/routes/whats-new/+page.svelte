<script lang="ts">
  const docs = 'https://vixide.github.io/vix/';

  // Every item is taken from the main repo's CHANGELOG.md ([Unreleased]) and
  // docs/whats-new-2-0/index.md; keep the three in agreement.
  const highlights = [
    {
      title: 'Remote editing over SSH',
      tag: 'experimental',
      text: 'vix ssh://user@host/path, or File → Open Remote…, opens a folder on another machine with nothing installed there. Search, Git and the terminal run on the remote machine; once you trust the workspace, so do language servers, the debugger and tasks. Docker and Podman containers and dev containers work the same way. Kubernetes and WSL are built but so far tested only against a stand-in.',
      href: `${docs}remote-editing/index.html`
    },
    {
      title: 'Cowork',
      tag: 'opt-in build',
      text: 'Edit the same files together in real time: single-use invites, the host approves each guest, everyone’s cursors in their own color, conflict-free merging, a session chat and a terminal shared read-only by default. Encrypted, with no server in between. Not in the default build or the prebuilt releases: everyone in a session needs a build made with the cowork feature.',
      href: `${docs}cowork/index.html`
    },
    {
      title: 'IM menu',
      tag: 'Matrix not built',
      text: 'Code-aware chat: a local Notes conversation, IRC (IRCv3 over TLS), path:line references that open on Enter, and threads anchored to lines of your files, stored in .vix/im/threads/ so Git can share them.',
      href: `${docs}im/index.html`
    },
    {
      title: 'IDE features',
      tag: '',
      text: 'Local History (a snapshot on every save, with diff and restore), Search Everywhere, a Refactor menu, Organize Imports and Code Cleanup, named run configurations, Peek Definition, CodeLens and postfix completion.',
      href: `${docs}ide-parity/index.html`
    },
    {
      title: 'Large File Viewer',
      tag: '',
      text: 'File → Open in Large File Viewer… reads a multi-gigabyte file read-only without loading it. On an Apple M4 Max with a warm cache, a 4 GiB log shows its first screen in under a millisecond and is fully indexed in about half a second, in under 5 MiB of memory (measured).',
      href: `${docs}large-file-viewer/index.html`
    },
    {
      title: 'Lighter everyday editing',
      tag: '',
      text: 'A 100 MB log now takes 126 MiB instead of 1.1 GiB, and an idle Vix uses no CPU instead of about 7.5% of a core. The image-support probe no longer delays the first screen.',
      href: `${docs}performance/index.html`
    }
  ];

  const more = [
    ['Containers', 'An OC menu for Docker, Podman and nerdctl: containers, images, volumes, networks, pods, Compose and builds. Every removing or running command shows its exact command first.'],
    ['AI when you want it', 'ai = "off" hides the AI menu and refuses every AI request. vix --mcp gives an agent a read-only view of a workspace. Inline ghost-text completion is off by default. A first slice of an Agent Client Protocol client works in trusted workspaces only.'],
    ['Editing', 'Multiple-caret depth, an Undo Tree, hot exit, safe saving that keeps hard links and owners, legacy encodings, and Compare Files side by side (vix --diff OLD NEW).'],
    ['Code intelligence and debugging', 'LSP status and progress, snippet completions, on-type formatting, call and type hierarchies, and a debugger with conditions, logpoints, exception filters, attach and threads.'],
    ['Git', 'An interactive commit graph, interactive rebase and cherry-pick, stash list and worktrees, a choosable change-gutter base and a read-only branch review.'],
    ['Terminal and tools', 'Several terminals with scrollback, Ctrl+click on path:line:col to open it, a REST client, a FHIR client, HL7 v2 view and MLLP send, a Todo Tree, your project’s own Prettier, and an opt-in Jupyter kernel.'],
    ['Project environments', 'A .venv, direnv or mise environment is applied to language servers, tasks and the terminal; direnv and mise run only in a workspace you have trusted.'],
    ['Windows on ARM', 'A native aarch64 .zip and .msi, alongside the Linux .deb, .rpm and Arch packages every release has carried since 1.8.0.']
  ];
</script>

<section class="section prose centered">
  <p class="section-heading-eyebrow">Not released yet. The current release is 1.8.</p>
  <h1>What’s new in Vix 2.0</h1>
  <p>
    2.0 takes Vix off your own machine and out of your own chair: edit a
    folder on another machine, edit the same file with other people, chat
    about code without leaving the editor, and open files too big for any
    editor to load. Everything below is built and tested; where something is
    experimental, opt-in or partial, it says so.
  </p>
  <p>
    The full record is the
    <a href="https://github.com/vixide/vix/blob/main/CHANGELOG.md">changelog</a>;
    the same notes are in the documentation as
    <a href={`${docs}whats-new-2-0/index.html`}>What’s new in 2.0</a>.
  </p>
</section>

<section class="section" aria-labelledby="highlights-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Highlights</p>
    <h2 id="highlights-heading">The big six</h2>
  </header>
  <div class="prose centered">
    <ul>
      {#each highlights as item (item.title)}
        <li>
          <strong><a href={item.href}>{item.title}</a></strong>{item.tag ? ` (${item.tag})` : ''} — {item.text}
        </li>
      {/each}
    </ul>
  </div>
</section>

<section class="section" aria-labelledby="more-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">And more</p>
    <h2 id="more-heading">Also new</h2>
  </header>
  <div class="prose centered">
    <ul>
      {#each more as [title, text] (title)}
        <li><strong>{title}</strong> — {text}</li>
      {/each}
    </ul>
    <p>
      <a href="/features/#new">The complete list is on the Features page.</a>
    </p>
  </div>
</section>

<section class="section" aria-labelledby="upgrading-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Before you upgrade</p>
    <h2 id="upgrading-heading">Upgrading</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li>
        <strong>The <code>vix</code> command and your settings files keep
        working.</strong> For example <code>auto_save = true</code> and
        <code>false</code> still load.
      </li>
      <li>
        <strong>Terminal:</strong> Tools → Terminal and <kbd>Ctrl+]</kbd> now
        hide the terminal, which keeps running; Tools → Terminal → Close
        Terminal stops it.
      </li>
      <li>
        <strong>Using the crates as a library:</strong> 2.0 is a major version
        because the <code>vix-*</code> crates made breaking API changes
        (typed errors, <code>#[non_exhaustive]</code> on types that keep
        growing, a smaller public surface on <code>App</code>) so later
        additions can be non-breaking. The changelog’s Breaking section lists
        each one.
      </li>
      <li><strong>Building from source</strong> needs Rust 1.97 or newer.</li>
    </ul>
  </div>
</section>

<section class="section" aria-labelledby="not-yet-heading">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Honest limits</p>
    <h2 id="not-yet-heading">Not in 2.0 yet</h2>
  </header>
  <div class="prose centered">
    <ul>
      <li>
        The winget, Scoop, AUR, <code>cargo binstall</code>, Nix, mise and
        container-image channels are not published or verified. Use one of the
        <a href="/install/">install routes that work today</a>.
      </li>
      <li>There is no apt or dnf repository; update by installing the next package.</li>
      <li>
        Matrix chat is planned, not built. Kubernetes and WSL workspaces are
        not yet tested against real systems.
      </li>
      <li>Cowork and Jupyter need a custom build.</li>
    </ul>
    <div class="button-row">
      <a class="button button-primary" href="/install/">Install Vix</a>
      <a class="button button-secondary" href="/features/#new">All new features</a>
    </div>
  </div>
</section>
