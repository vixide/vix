//! The mode enum (`spec/index.md`, § Design: modes).

/// A modal-editing mode. Four for v1 — Visual Block is cut (see the spec's
/// "v1 cut line").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Commands act on motions/text objects; the default mode.
    #[default]
    Normal,
    /// Ordinary text insertion, exactly like every other keymap's typing.
    Insert,
    /// Character-wise selection.
    Visual,
    /// Line-wise selection.
    VisualLine,
}

impl Mode {
    /// The short, upper-case label real Vim's own status line shows for this
    /// mode (`"NORMAL"` is conventionally left implicit in Vim's own status
    /// line, but Vix always shows one, for a learner who hasn't memorized
    /// "blank means Normal" yet).
    #[must_use]
    pub fn status_label(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mode;

    #[test]
    fn default_mode_is_normal() {
        assert_eq!(Mode::default(), Mode::Normal);
    }

    #[test]
    fn every_mode_has_a_distinct_label() {
        let modes = [Mode::Normal, Mode::Insert, Mode::Visual, Mode::VisualLine];
        let labels: Vec<&str> = modes.iter().map(|m| m.status_label()).collect();
        for (i, a) in labels.iter().enumerate() {
            for (j, b) in labels.iter().enumerate() {
                assert!(i == j || a != b, "labels must be unique: {a} vs {b}");
            }
        }
    }
}
