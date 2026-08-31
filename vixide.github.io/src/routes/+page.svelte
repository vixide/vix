<script lang="ts">
  import { Card } from 'lily-design-system-svelte-headless';
</script>

<svelte:head>
  <meta
    name="description"
    content="Vix is a keyboard-friendly terminal text editor built on ratatui, with a file explorer, LSP, Git integration, a database workbench, Org mode, and much more."
  />
</svelte:head>

<section class="hero">
  <h1>A keyboard-friendly terminal text editor.</h1>
  <p class="hero-tagline">
    Vix is a high-speed, high-security integrated development environment that
    lives entirely in your terminal — a file explorer, tree-sitter syntax
    highlighting, LSP, a database workbench, Git, Org mode, and much more,
    built on <a href="https://ratatui.rs/">ratatui</a> in Rust.
  </p>
  <div class="button-row">
    <a class="button button-primary" href="/install/">Install Vix</a>
    <a class="button button-secondary" href="/features/">Browse features</a>
    <a class="button button-secondary" href="https://github.com/vixide/vix">View on GitHub</a>
  </div>
</section>

<section class="section">
  <div class="prose" style="margin: 0 auto;">
    <pre class="terminal-mock"><code
      >{`Vix  File  Edit  View  Go  Run  AI  DB  Git  Org  Tools  Help
╭─Explorer-──╮╭ main.rs ──────────────────╮╭Messages────────╮
│ README.md  ││ 1  fn main() {            ││ Welcome to Vix │
│ src        ││ 2      println!("hello"); ││ Ctrl+B toggles │
│   main.rs  ││ 3  }                      ││                │
╰────────────╯╰───────────────────────────╯╰────────────────╯
╭─Output────────────────────────────────────────────────────╮
│ hello                                                      │
╰──────────────────────────────────────────────────────────╯
src/main.rs — Ready              main • text UTF-8 Ln 2:Col 5`}</code
    ></pre>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Why Vix</p>
    <h2>Everything you reach for, without leaving the terminal</h2>
  </header>

  <div class="card-grid">
    <Card heading="A real Cargo workspace" href="https://github.com/vixide/vix">
      <p class="card-description">
        A thin App shell over more than 100 focused <code>vix-*</code> member
        crates — one crate, one job, each with its own spec.
      </p>
    </Card>
    <Card heading="Ten keymaps" href="/features/">
      <p class="card-description">
        Apple, VS Code (macOS/Windows), Emacs, Vi, Spacemacs, IntelliJ
        (macOS/Windows), Eclipse, and Sublime Text — switch anytime.
      </p>
    </Card>
    <Card heading="LSP, DAP, and Git" href="/features/">
      <p class="card-description">
        Diagnostics, completion, go-to-definition, and rename; breakpoints and
        a stepping debugger; stage, diff, blame, and conflict resolution — all
        built in.
      </p>
    </Card>
    <Card heading="A database workbench" href="/features/">
      <p class="card-description">
        Connect to SQLite, PostgreSQL, or MySQL, browse the schema, run
        queries with autocomplete, and read results in a filterable grid.
      </p>
    </Card>
    <Card heading="Org mode" href="/features/">
      <p class="card-description">
        Headline promote/demote, TODO cycling, a structural table editor with
        <code>TBLFM</code> formulas, an interactive Column View, and Org-roam.
      </p>
    </Card>
    <Card heading="A Project menu" href="/features/">
      <p class="card-description">
        Per-project-type lifecycle commands, task discovery across seven
        build tools, monorepo subprojects, and test-at-point.
      </p>
    </Card>
  </div>
</section>

<section class="section">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Quick start</p>
    <h2>Try Vix in under a minute</h2>
  </header>

  <div class="prose" style="margin: 0 auto;">
    <p>Requires a Rust toolchain (1.96+):</p>
    <pre><code
      >{`cargo run                  # open the editor rooted at the current directory
cargo run -- src/main.rs   # open one or more files on launch
cargo build --release      # optimized binary at target/release/vix`}</code
    ></pre>
    <p style="text-align: center; margin-top: 2rem;">
      <a class="button button-primary" href="/install/">Full install instructions →</a>
    </p>
  </div>
</section>

<section class="section prose" style="margin: 0 auto;">
  <header class="section-heading">
    <p class="section-heading-eyebrow">Open source</p>
    <h2>Free, and built in the open</h2>
  </header>
  <p>
    Vix is licensed under your choice of Apache-2.0, BSD-3-Clause, MIT,
    GPL-2.0, or GPL-3.0. Development is
    specification-driven: every crate owns a spec at
    <code>crates/&lt;crate&gt;/spec/index.md</code>, and the code implements it.
  </p>
  <p style="text-align: center; margin-top: 1.5rem;">
    <a class="button button-secondary" href="https://github.com/vixide/vix/blob/main/CHANGELOG.md"
      >Read the changelog →</a
    >
  </p>
</section>
