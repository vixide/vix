//! The numeric-prefix accumulator (`spec/index.md`, § Design: counts).

/// An in-progress or resolved numeric prefix (`3` before `dw`, `5j`, a bare
/// motion with no digits at all). `None` inside means "no digits typed yet",
/// distinct from an explicit `1` — both report [`Count::value`] as `1`, but
/// [`Count::is_empty`] tells them apart, which matters for deciding whether a
/// leading `0` is the `0` motion (no digits yet) or the tenths digit of `10`
/// (a count already in progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Count(Option<usize>);

impl Count {
    /// Feed one more digit onto the accumulating count (`push_digit('3')`
    /// then `push_digit('14')`'s first char... — callers pass one `char` at a
    /// time as keys arrive). Returns `false` and leaves `self` unchanged for
    /// anything that isn't an ASCII digit, or for a leading `'0'` when no
    /// digits have been typed yet (that's the `0` motion, not a count — real
    /// Vim's own rule; see the spec's § Design: counts).
    pub fn push_digit(&mut self, c: char) -> bool {
        let Some(d) = c.to_digit(10) else {
            return false;
        };
        if d == 0 && self.0.is_none() {
            return false;
        }
        let next = self
            .0
            .unwrap_or(0)
            .saturating_mul(10)
            .saturating_add(d as usize);
        self.0 = Some(next);
        true
    }

    /// The effective count: `1` when no digits were typed, matching every
    /// motion's default of "once".
    #[must_use]
    pub fn value(self) -> usize {
        self.0.unwrap_or(1)
    }

    /// Whether any digit has been typed yet (as opposed to an explicit `1`).
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0.is_none()
    }

    /// Vim's `{count1}{operator}{count2}{motion}` composition rule: the two
    /// counts multiply (`2d3w` deletes 6 words), each defaulting to `1` when
    /// absent.
    #[must_use]
    pub fn combine(self, other: Count) -> usize {
        self.value().saturating_mul(other.value())
    }

    /// Clear back to "no digits typed" — every mode-affecting key (a motion
    /// firing, `Esc`, entering Insert, …) resets the count.
    pub fn reset(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::Count;

    #[test]
    fn default_count_is_empty_and_reports_value_one() {
        let c = Count::default();
        assert!(c.is_empty());
        assert_eq!(c.value(), 1);
    }

    #[test]
    fn digits_accumulate_left_to_right() {
        let mut c = Count::default();
        assert!(c.push_digit('3'));
        assert!(c.push_digit('4'));
        assert_eq!(c.value(), 34);
        assert!(!c.is_empty());
    }

    #[test]
    fn a_leading_zero_is_not_a_digit_of_a_count() {
        let mut c = Count::default();
        assert!(!c.push_digit('0'), "leading 0 is the 0 motion, not a count");
        assert!(c.is_empty());
    }

    #[test]
    fn a_zero_after_other_digits_is_a_digit() {
        let mut c = Count::default();
        assert!(c.push_digit('1'));
        assert!(c.push_digit('0'));
        assert_eq!(c.value(), 10);
    }

    #[test]
    fn non_digit_chars_are_rejected() {
        let mut c = Count::default();
        assert!(!c.push_digit('w'));
        assert!(c.is_empty());
    }

    #[test]
    fn combine_multiplies_both_counts_defaulting_to_one() {
        let mut op = Count::default();
        op.push_digit('2');
        let mut motion = Count::default();
        motion.push_digit('3');
        assert_eq!(op.combine(motion), 6, "2d3w deletes 6 words");
        assert_eq!(Count::default().combine(Count::default()), 1);
        assert_eq!(op.combine(Count::default()), 2);
    }

    #[test]
    fn reset_clears_back_to_empty() {
        let mut c = Count::default();
        c.push_digit('9');
        c.reset();
        assert!(c.is_empty());
        assert_eq!(c.value(), 1);
    }
}
