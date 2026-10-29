//! Peers and what each is allowed to do.

use crate::error::{CoworkError, WireProblem};

/// A participant's identity inside one session.
///
/// It is also the CRDT peer id: every operation a participant authors carries
/// it. The host assigns one to each guest at join and never reuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PeerId(pub u64);

impl PeerId {
    /// The host is always peer 1.
    pub const HOST: Self = Self(1);

    /// The id handed to the nth guest to join (the first guest is peer 2).
    #[must_use]
    pub const fn guest(index: u64) -> Self {
        Self(index.saturating_add(2))
    }

    /// A small stable index (for choosing a palette color): the id itself.
    #[must_use]
    pub const fn index(self) -> u64 {
        self.0
    }
}

/// What a guest may do. Ordered: each level includes the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[non_exhaustive]
pub enum Permission {
    /// Watch only. The default for every new guest.
    #[default]
    View,
    /// Edit shared buffers and ask the host to save.
    Edit,
    /// Edit, and take part in the shared terminal.
    EditTerminal,
}

/// Something a peer asks to do, for [`Permission::allows`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Read buffer text and see presence.
    Read,
    /// Publish a text edit.
    Edit,
    /// Ask the host to write a buffer to disk.
    Save,
    /// Type into the shared terminal.
    Terminal,
}

impl Permission {
    /// Whether this level permits `action`.
    #[must_use]
    pub const fn allows(self, action: Action) -> bool {
        match action {
            Action::Read => true,
            Action::Edit | Action::Save => matches!(self, Self::Edit | Self::EditTerminal),
            Action::Terminal => matches!(self, Self::EditTerminal),
        }
    }

    /// Check `action`, returning [`CoworkError::PermissionDenied`] if refused.
    ///
    /// # Errors
    ///
    /// When this level does not permit `action`.
    pub const fn require(self, action: Action) -> Result<(), CoworkError> {
        if self.allows(action) {
            Ok(())
        } else {
            Err(CoworkError::PermissionDenied)
        }
    }

    /// The wire byte for this level.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::View => 0,
            Self::Edit => 1,
            Self::EditTerminal => 2,
        }
    }

    /// Decode a wire byte.
    ///
    /// # Errors
    ///
    /// When the byte names no level.
    pub const fn from_byte(b: u8) -> Result<Self, WireProblem> {
        match b {
            0 => Ok(Self::View),
            1 => Ok(Self::Edit),
            2 => Ok(Self::EditTerminal),
            other => Err(WireProblem::UnknownPermission(other)),
        }
    }
}

/// One connected participant as the host sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// The session identity.
    pub id: PeerId,
    /// The display name the host confirmed at join.
    pub name: String,
    /// What this peer may do.
    pub permission: Permission,
}

impl Peer {
    /// A new peer with the safe default permission ([`Permission::View`]).
    #[must_use]
    pub fn new(id: PeerId, name: &str) -> Self {
        Self {
            id,
            name: sanitize_name(name),
            permission: Permission::View,
        }
    }
}

/// Longest display name kept, in characters.
pub const MAX_NAME_CHARS: usize = 32;

/// Make a guest-supplied display name safe to render: control characters
/// (terminal escape sequences!) are dropped, whitespace is trimmed, the length
/// is capped, and an empty result becomes `guest`.
#[must_use]
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "guest".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_levels_are_ordered_and_default_is_view() {
        assert!(Permission::View < Permission::Edit);
        assert!(Permission::Edit < Permission::EditTerminal);
        assert_eq!(Permission::default(), Permission::View);
        assert_eq!(
            Peer::new(PeerId::guest(0), "x").permission,
            Permission::View
        );
    }

    #[test]
    fn view_cannot_edit_save_or_type() {
        for a in [Action::Edit, Action::Save, Action::Terminal] {
            assert_eq!(
                Permission::View.require(a),
                Err(CoworkError::PermissionDenied)
            );
        }
        assert!(Permission::View.allows(Action::Read));
    }

    #[test]
    fn edit_cannot_use_the_terminal() {
        assert!(Permission::Edit.allows(Action::Edit));
        assert!(Permission::Edit.allows(Action::Save));
        assert!(!Permission::Edit.allows(Action::Terminal));
        assert!(Permission::EditTerminal.allows(Action::Terminal));
    }

    #[test]
    fn permission_byte_round_trips_and_rejects_unknown() {
        for p in [Permission::View, Permission::Edit, Permission::EditTerminal] {
            assert_eq!(Permission::from_byte(p.to_byte()), Ok(p));
        }
        assert_eq!(
            Permission::from_byte(9),
            Err(WireProblem::UnknownPermission(9))
        );
    }

    #[test]
    fn names_are_stripped_of_escape_sequences_and_capped() {
        assert_eq!(sanitize_name("\u{1b}[31mred\u{7}"), "[31mred");
        assert_eq!(sanitize_name("   "), "guest");
        assert_eq!(
            sanitize_name(&"x".repeat(100)).chars().count(),
            MAX_NAME_CHARS
        );
    }

    #[test]
    fn guest_ids_never_collide_with_the_host() {
        assert_ne!(PeerId::guest(0), PeerId::HOST);
        assert_eq!(PeerId::guest(0), PeerId(2));
    }
}
