//! `vix-matrix`: the Matrix helper for Vix's IM (`spec/index.md`).
//!
//! Vix starts this program and talks to it over JSON lines: requests on
//! stdin, replies on stdout, with the protocol of the `vix-matrix-protocol` crate
//! (`crates/vix-matrix-protocol`, `spec/instant-message/matrix.md` at the repo
//! root). It is a `matrix-sdk` client with an encrypted SQLite store and
//! end-to-end encryption; it runs out of process because `matrix-sdk`'s
//! SQLite cannot share a lockfile with Vix's.
//!
//! stdout carries reply lines and nothing else. Diagnostics go to stderr,
//! at `warn` and above only, and never include message bodies, tokens, keys
//! or passwords.

mod events;
mod lines;
mod secrets;
mod serve;
mod session;
mod store;

use std::io::BufReader;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use tracing::level_filters::LevelFilter;

fn main() -> ExitCode {
    // `ring`, not aws-lc, as the rustls provider for every TLS connection
    // (`matrix-sdk` builds reqwest with `rustls-no-provider`). An error
    // means a provider is already installed, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
    // stderr only, capped at `warn`, whatever `RUST_LOG` says: the SDK's
    // debug and trace output can include event contents.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(LevelFilter::WARN)
        .init();
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    else {
        tracing::error!("cannot start the async runtime");
        return ExitCode::FAILURE;
    };
    let env = serve::Env {
        secrets: Arc::new(secrets::OsKeyring::new()),
        data_dir: store::data_dir(),
    };
    runtime.block_on(serve::serve(
        BufReader::new(std::io::stdin()),
        std::io::stdout(),
        env,
    ));
    // Don't wait on SDK background work (or the stdin thread) after Closed.
    runtime.shutdown_timeout(Duration::from_secs(2));
    ExitCode::SUCCESS
}
