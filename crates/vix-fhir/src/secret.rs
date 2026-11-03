//! A string that must never be printed.
//!
//! Used for bearer tokens and OAuth client secrets throughout the crate. The
//! type deliberately offers no `Display`/`Deref`/`AsRef<str>`, so leaking it
//! requires an explicit [`Secret::expose`] call that is easy to grep for.
//! Note it does not zeroize memory on drop. Spec: `crates/vix-fhir/spec/index.md`.

/// A credential (token or client secret). Its `Debug` output is redacted and it
/// has no `Display`, so it cannot reach a log or message by accident; the only
/// way to read it is the explicit [`Secret::expose`].
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a credential.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The credential itself — call only where it goes on the wire.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("hunter2".into());
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(s.expose(), "hunter2");
    }
}
