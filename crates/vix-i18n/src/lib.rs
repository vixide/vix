//! Shared internationalization backend for the Vix workspace.
//!
//! `rust_i18n`'s `t!` macro expands to `crate::_rust_i18n_t!`, and the `i18n!`
//! macro embeds the whole translation table into whichever crate invokes it.
//! In a multi-crate workspace that would embed `locales/app.yml` once per crate.
//! To avoid that, this crate invokes `i18n!` exactly once and re-exposes a
//! drop-in [`t!`] plus a [`surface!`] helper; consumer crates call
//! `vix_i18n::surface!()` at their root and use `vix_i18n::t!` (or bring it in
//! unqualified with `#[macro_use] extern crate vix_i18n;`). The full API is
//! documented because the workspace forbids missing docs.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

// Re-export rust_i18n so `$crate::rust_i18n::_tr!` resolves inside the `t!`
// expansion below without the consumer needing rust_i18n in scope by that path.
#[doc(hidden)]
pub extern crate rust_i18n;

// The one and only embed of the workspace translation table. The path is
// resolved relative to this crate's manifest, so it points at the repo-root
// `locales/` directory shared by the whole project.
rust_i18n::i18n!("../../locales", fallback = "en");

#[doc(no_inline)]
pub use rust_i18n::{available_locales, locale, set_locale};

/// Translate a key against the shared locale table.
///
/// A drop-in replacement for [`rust_i18n::t!`] that resolves against this
/// crate's single embedded table instead of a per-crate one. Supports the same
/// forms: `t!("key")`, `t!("key", name = value)`, `t!("key", count = n)`, and
/// `t!("key", locale = "es")`.
#[macro_export]
macro_rules! t {
    ($($all:tt)*) => {
        $crate::rust_i18n::_tr!(
            $($all)*,
            _minify_key = false,
            _minify_key_len = 0,
            _minify_key_prefix = "t_",
            _minify_key_thresh = 0
        )
    };
}

/// Surface the translation lookup functions at the calling crate's root.
///
/// The `t!` expansion references `crate::_rust_i18n_try_translate`, which must
/// resolve in the crate where the translation happens. Invoke this once at the
/// root of every crate that uses [`t!`].
#[macro_export]
macro_rules! surface {
    () => {
        #[doc(hidden)]
        pub use $crate::{_rust_i18n_translate, _rust_i18n_try_translate};
    };
}
