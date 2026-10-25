<script lang="ts">
  import CopyCommand from '#lib/CopyCommand.svelte';

  const docs = 'https://vixide.github.io/vix/';
  const release = 'https://github.com/vixide/vix/releases/latest/download';
</script>

<svelte:head>
  <meta
    name="description"
    content="Install Vix with Homebrew, an installer script, PowerShell, npm, or cargo — or download a prebuilt or minimal build, or a desktop GUI."
  />
</svelte:head>

<section class="section prose centered">
  <h1>Install Vix</h1>
  <p>
    Pick whichever fits how you work — they all install the same program, and
    the command is <code>vix</code> however you install it. Only building
    from source needs a Rust toolchain.
  </p>

  <h2 id="homebrew">Homebrew (macOS, Linux)</h2>
  <CopyCommand command="brew install vixide/homebrew-tap/vix-tui" label="Copy the Homebrew install command" />

  <h2 id="script">Installer script (macOS, Linux)</h2>
  <p>Downloads the right prebuilt archive for your platform and puts <code>vix</code> on your <code>PATH</code>.</p>
  <CopyCommand
    command={`curl --proto '=https' --tlsv1.2 -LsSf ${release}/vix-tui-installer.sh | sh`}
    label="Copy the installer script command"
  />

  <h2 id="linux">Linux packages (apt, dnf, pacman)</h2>
  <p>
    Every release carries native packages of <code>vix-tui</code> and the
    desktop GUIs for x86_64 and aarch64. Download the one for your
    distribution from the
    <a href="https://github.com/vixide/vix/releases/latest">latest release</a>,
    then install it:
  </p>
  <CopyCommand command="sudo apt install ./vix-tui_*_amd64.deb" label="Copy the Debian and Ubuntu install command" />
  <CopyCommand command="sudo dnf install ./vix-tui-*.x86_64.rpm" label="Copy the Fedora and RHEL install command" />
  <CopyCommand command="sudo pacman -U ./vix-tui-*-x86_64.pkg.tar.zst" label="Copy the Arch Linux install command" />
  <p>
    They need glibc 2.35 or newer (Ubuntu 22.04+, Debian 12+, Fedora 36+,
    RHEL/Rocky 10). There is no apt or dnf repository yet, so updating means
    installing the next release's package.
  </p>

  <h2 id="windows">Windows (PowerShell)</h2>
  <CopyCommand
    command={`powershell -ExecutionPolicy Bypass -c "irm ${release}/vix-tui-installer.ps1 | iex"`}
    label="Copy the PowerShell install command"
  />

  <h2 id="npm">npm (any platform npm supports)</h2>
  <CopyCommand command="npm install -g @vixide/vix" label="Copy the npm install command" />

  <h2 id="cargo">Cargo (any platform with Rust 1.96 or newer)</h2>
  <CopyCommand command="cargo install vix-tui" label="Copy the cargo install command" />

  <h2 id="download">Direct download</h2>
  <p>
    Prebuilt archives — <code>.tar.xz</code> for macOS and Linux,
    <code>.msi</code> and <code>.zip</code> for Windows — are attached to every
    <a href="https://github.com/vixide/vix/releases">GitHub Release</a>.
  </p>

  <h2 id="minimal">Minimal build</h2>
  <p>
    Every release also carries <code>vix-minimal-&lt;target&gt;</code>, a small,
    locked-down build. It leaves out the database workbench, image preview,
    scripting, the integrated terminal, the HTTP client, the AI assistant,
    Git and Jujutsu, language servers, debug adapters, the task runner and
    syntax highlighting. It links no TLS stack, makes no network connections,
    and Vix starts no other program itself. Unpack it and put <code>vix</code>
    on your <code>PATH</code>, or build it yourself:
  </p>
  <CopyCommand command="cargo build --profile minimal --no-default-features" label="Copy the minimal build command" />

  <h2 id="desktop">Desktop GUIs</h2>
  <p>
    <code>vix-gpui</code>, <code>vix-egui</code>, <code>vix-iced</code> and
    <code>vix-slint</code> ship from the same release and install the same
    way under their own names. They are experimental; <code>vix-slint</code>
    is GPL-3.0-only, and <code>vix-gtk4</code> is source-only for now. See the
    <a href="https://github.com/vixide/vix/blob/main/gui/spec/index.md">GUI spec</a>
    for platforms and run-time libraries.
  </p>
  <CopyCommand command="brew install vixide/homebrew-tap/vix-gpui" label="Copy the vix-gpui Homebrew install command" />

  <h2 id="source">Build from source</h2>
  <pre><code
    >{`git clone https://github.com/vixide/vix.git
cd vix
cargo run                                    # open the editor here
cargo build --release                        # binary at target/release/vix
cargo build --release --features syntax-all  # with every Tree-sitter grammar`}</code
  ></pre>
  <p>
    The default build compiles a lean set of Tree-sitter grammars (Rust,
    Markdown, JSON, TOML), since each is a sizeable compiled C parser.
  </p>

  <h2 id="first-run">First run</h2>
  <pre><code
    >{`vix                     # open the current directory
vix src/main.rs:42:7    # open a file at line 42, column 7
vix --locale fr         # start in French
vix --help              # every command-line option`}</code
  ></pre>
  <p>
    Press <kbd>Esc</kbd> to close the welcome screen, then <kbd>Ctrl+P</kbd>
    for the command palette and <kbd>F1</kbd> for every shortcut. Use a
    <a href="https://www.nerdfonts.com/">Nerd Font</a> in your terminal for the
    file and folder icons; Vix falls back to plain characters without one.
  </p>
  <p>
    Settings are TOML in your platform's configuration directory (for example
    <code>~/.config/vix/config.toml</code> on Linux), and custom themes go in
    <code>~/.config/vix/themes/*.json</code>. See
    <a href={`${docs}getting-started/index.html`}>Getting Started</a> and
    <a href={`${docs}configuration/index.html`}>Configuration</a>.
  </p>

  <div class="button-row">
    <a class="button button-primary" href={`${docs}getting-started/index.html`}>Read Getting Started</a>
    <a class="button button-secondary" href="/features/">See all features</a>
  </div>
</section>
