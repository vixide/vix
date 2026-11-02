<svelte:head>
  <meta name="description" content="How to install Vix with Homebrew, npm, an installer script, native packages, or from source." />
</svelte:head>

<section class="section prose" style="margin: 0 auto;">
  <h1>Install</h1>

  <p>
    The command is <code>vix</code> however you install it. Pick whichever
    suits your platform; prebuilt binaries need no Rust toolchain.
  </p>

  <h2>Install a prebuilt binary</h2>
  <pre><code
    >{`# macOS or Linux, Homebrew
brew install vixide/homebrew-tap/vix-tui

# macOS or Linux, installer script
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vixide/vix/releases/latest/download/vix-tui-installer.sh | sh

# Any platform with npm
npm install -g @vixide/vix

# Any platform with a Rust toolchain (1.97+)
cargo install vix-tui`}</code
  ></pre>
  <p>
    On Windows, use PowerShell:
  </p>
  <pre><code
    >{`powershell -ExecutionPolicy Bypass -c "irm https://github.com/vixide/vix/releases/latest/download/vix-tui-installer.ps1 | iex"`}</code
  ></pre>
  <p>
    On Linux, every release also has native packages (<code>.deb</code>,
    <code>.rpm</code>, Arch <code>.pkg.tar.zst</code>) for x86_64 and aarch64
    on the
    <a href="https://github.com/vixide/vix/releases/latest">latest release</a>
    page. Prebuilt archives (<code>.tar.xz</code>, <code>.zip</code>,
    <code>.msi</code>) are attached to every
    <a href="https://github.com/vixide/vix/releases">GitHub Release</a>,
    along with a <strong>minimal build</strong> (<code>vix-minimal-&lt;target&gt;</code>)
    that leaves out every network, shell and external-program feature.
  </p>

  <h2>Requirements</h2>
  <ul>
    <li>To build from source: a Rust toolchain, <strong>1.97 or newer</strong>.</li>
    <li>A <a href="https://www.nerdfonts.com/">Nerd Font</a>, for best results — it supplies the file/folder/clock glyphs (Vix falls back to plain characters without one).</li>
  </ul>

  <h2>Build from source</h2>
  <pre><code
    >{`git clone https://github.com/vixide/vix
cd vix
cargo run                  # open the editor rooted at the current directory
cargo run -- src/main.rs   # open one or more files on launch
cargo run -- file.rs:42:7  # open and jump straight to line 42, column 7
cargo run -- --locale fr   # start in French (overrides the saved language)
cargo build --release      # optimized binary at target/release/vix
vix --help                 # full CLI usage`}</code
  ></pre>

  <h2>Syntax highlighting grammars</h2>
  <p>
    Each Tree-sitter grammar is a sizeable compiled C parser, so they're
    gated behind Cargo features. The default build compiles a lean set (Rust,
    Markdown, JSON, TOML):
  </p>
  <pre><code
    >{`cargo build --release                                              # common grammars (default)
cargo build --release --no-default-features                        # no highlighting (smallest)
cargo build --release --no-default-features --features syntax-all  # all grammars (largest)`}</code
  ></pre>

  <h2>Configuration</h2>
  <p>
    Settings are stored as TOML in the platform configuration directory
    (e.g. <code>~/.config/vix/config.toml</code> on Linux). Custom themes live
    alongside it in <code>~/.config/vix/themes/*.json</code>.
  </p>

  <h2>License</h2>
  <p>
    Licensed under your choice of Apache-2.0, BSD-3-Clause, MIT, GPL-2.0, or
    GPL-3.0.
  </p>

  <p style="text-align: center; margin-top: 2rem;">
    <a class="button button-primary" href="https://github.com/vixide/vix">View the source on GitHub →</a>
  </p>
</section>
