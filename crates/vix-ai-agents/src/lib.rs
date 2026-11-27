//! Native coding agents available through the Vix AI menu.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

pub mod conversation;
pub mod protocol;

use serde::{Deserialize, Serialize};

/// A coding agent with a native CLI integration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    /// The Codex command-line agent.
    Codex,
    /// Anthropic Claude Code.
    Claude,
}

impl Provider {
    /// The installed executable that owns the user's login and sessions.
    #[must_use]
    pub const fn executable(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    /// The provider's product name in the chooser and conversation heading.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
        }
    }
}
