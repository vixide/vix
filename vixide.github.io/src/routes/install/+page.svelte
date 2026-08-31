<svelte:head>
  <meta name="description" content="How to build and run Vix from source." />
</svelte:head>

<section class="section prose" style="margin: 0 auto;">
  <h1>Install</h1>

  <p>
    Vix isn't packaged for a registry yet — build it from source with a Rust
    toolchain. It's one crate in a Cargo workspace, so there's nothing else to
    install first.
  </p>

  <h2>Requirements</h2>
  <ul>
    <li>A Rust toolchain, <strong>1.96 or newer</strong> (the MSRV is the current stable release minus two).</li>
    <li>A <a href="https://www.nerdfonts.com/">Nerd Font</a>, for best results — it supplies the file/folder/clock glyphs.</li>
  </ul>

  <h2>Build and run</h2>
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
