//! Resolving a set of override *requests* (persisted user overrides,
//! script `bind_key` requests) against each other and against the
//! built-in tables — the logic behind T104i's `App::override_key` choke
//! point. See `spec/index.md`'s "Override layer" and "Conflict handling".
//!
//! Deliberately pure and keymap-parameterized (`resolve` takes
//! `keymap_id` as a plain `&str`, no `App` dependency at all) so it's
//! fully unit-testable here rather than only exercisable through
//! `tests/integration.rs`.

#![warn(clippy::pedantic)]

use std::collections::BTreeMap;

/// Where an override request came from — for reporting, not for
/// precedence: a conflict between two requests is reported and rejected,
/// never silently resolved by one source outranking the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A loaded script's own `bind_key` request, naming the script's file
    /// stem (`vix-script`'s `LoadedScript::stem`).
    Script(String),
    /// The persisted `keybindings.toml`.
    User,
}

impl Source {
    /// A short, human-readable description of this source, for messages.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Source::Script(stem) => format!("script '{stem}'"),
            Source::User => "your keybindings.toml".to_string(),
        }
    }
}

/// One requested override: rebind `key_token` (`vix-macros` grammar,
/// always the top-level `""` context — neither source has any notion of
/// a chord) to `action_id`, tagged with where the request came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    /// The key token being rebound, in `vix-macros`' grammar.
    pub key_token: String,
    /// The `App::run_action`-dispatchable id this key should run instead.
    pub action_id: String,
    /// Where this request came from.
    pub source: Source,
}

/// Two or more override requests claimed the same `key_token`: none of
/// them wins, and every request naming that token is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The token every listed request claimed.
    pub key_token: String,
    /// Every source that claimed it, in the order the requests arrived.
    pub sources: Vec<Source>,
}

/// An accepted override (no conflict) that happens to also claim a token
/// a built-in binding in the resolved keymap already owns. Not a
/// conflict — the override still wins — but worth telling the user or
/// script author about, once, so the built-in's silence isn't a surprise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadow {
    /// The shared token.
    pub key_token: String,
    /// The override's own action id (what actually runs now).
    pub action_id: String,
    /// Where the winning override came from.
    pub source: Source,
    /// The built-in action id the override shadows.
    pub shadowed_action_id: &'static str,
}

/// The result of [`resolve`]ing a batch of override requests.
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    /// Every request that won: no other request claimed its token.
    /// Deterministically sorted by `key_token`.
    pub accepted: Vec<Override>,
    /// Every token two or more requests claimed, with none of them
    /// applied. Deterministically sorted by `key_token`.
    pub conflicts: Vec<Conflict>,
    /// Every accepted override that also shadows a built-in binding.
    /// Deterministically sorted by `key_token`.
    pub shadows: Vec<Shadow>,
}

/// Resolve `requests` against each other (same token claimed twice or
/// more → both/all rejected) and, for the survivors, against
/// `keymap_id`'s built-in table (already claimed → not a conflict, but
/// reported as a [`Shadow`]). Grouped in a [`BTreeMap`] rather than a
/// `HashMap` so the result — and therefore every message built from it —
/// is deterministic across runs, not just correct.
#[must_use]
pub fn resolve(requests: Vec<Override>, keymap_id: &str) -> Resolved {
    let mut by_token: BTreeMap<String, Vec<Override>> = BTreeMap::new();
    for req in requests {
        by_token.entry(req.key_token.clone()).or_default().push(req);
    }
    let mut accepted = Vec::new();
    let mut conflicts = Vec::new();
    for (key_token, mut group) in by_token {
        if group.len() > 1 {
            let sources = group.into_iter().map(|o| o.source).collect();
            conflicts.push(Conflict { key_token, sources });
        } else if let Some(only) = group.pop() {
            accepted.push(only);
        }
    }
    let shadows = accepted
        .iter()
        .filter_map(|o| {
            crate::lookup(keymap_id, "", &o.key_token).map(|shadowed_action_id| Shadow {
                key_token: o.key_token.clone(),
                action_id: o.action_id.clone(),
                source: o.source.clone(),
                shadowed_action_id,
            })
        })
        .collect();
    Resolved {
        accepted,
        conflicts,
        shadows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(key_token: &str, action_id: &str) -> Override {
        Override {
            key_token: key_token.to_string(),
            action_id: action_id.to_string(),
            source: Source::User,
        }
    }

    #[test]
    fn a_lone_request_is_accepted() {
        let resolved = resolve(vec![user("C-S-k", "edit.duplicate_line")], "emacs");
        assert_eq!(resolved.accepted.len(), 1);
        assert!(resolved.conflicts.is_empty());
    }

    #[test]
    fn two_requests_for_the_same_token_are_both_rejected() {
        let resolved = resolve(
            vec![
                user("C-S-k", "edit.duplicate_line"),
                Override {
                    key_token: "C-S-k".to_string(),
                    action_id: "edit.select_line".to_string(),
                    source: Source::Script("demo".to_string()),
                },
            ],
            "emacs",
        );
        assert!(resolved.accepted.is_empty());
        assert_eq!(resolved.conflicts.len(), 1);
        assert_eq!(resolved.conflicts[0].key_token, "C-S-k");
        assert_eq!(
            resolved.conflicts[0].sources,
            vec![Source::User, Source::Script("demo".to_string())]
        );
    }

    #[test]
    fn requests_for_different_tokens_do_not_conflict() {
        let resolved = resolve(
            vec![user("C-S-k", "a.action"), user("C-j", "b.action")],
            "emacs",
        );
        assert_eq!(resolved.accepted.len(), 2);
        assert!(resolved.conflicts.is_empty());
    }

    #[test]
    fn an_override_shadowing_a_builtin_is_accepted_and_reported() {
        // "C-s" is Emacs's built-in find binding (top-level context).
        let resolved = resolve(vec![user("C-s", "edit.query_replace")], "emacs");
        assert_eq!(resolved.accepted.len(), 1);
        assert_eq!(resolved.shadows.len(), 1);
        assert_eq!(resolved.shadows[0].key_token, "C-s");
        assert_eq!(resolved.shadows[0].shadowed_action_id, "edit.find");
        assert_eq!(resolved.shadows[0].action_id, "edit.query_replace");
    }

    #[test]
    fn an_override_of_an_unbound_token_is_not_a_shadow() {
        let resolved = resolve(vec![user("C-does-not-exist", "some.action")], "emacs");
        assert_eq!(resolved.accepted.len(), 1);
        assert!(resolved.shadows.is_empty());
    }

    #[test]
    fn a_rejected_conflict_is_never_also_reported_as_a_shadow() {
        // Both requests target "C-s" (Emacs's built-in save) -- the
        // conflict rejects both, so neither should show up as an
        // accepted-and-shadowing override too.
        let resolved = resolve(
            vec![user("C-s", "a.action"), user("C-s", "b.action")],
            "emacs",
        );
        assert!(resolved.accepted.is_empty());
        assert!(resolved.shadows.is_empty());
        assert_eq!(resolved.conflicts.len(), 1);
    }

    #[test]
    fn resolution_is_deterministic_regardless_of_request_order() {
        let a = resolve(
            vec![user("C-j", "b.action"), user("C-a", "a.action")],
            "emacs",
        );
        let b = resolve(
            vec![user("C-a", "a.action"), user("C-j", "b.action")],
            "emacs",
        );
        assert_eq!(a.accepted, b.accepted);
    }
}
