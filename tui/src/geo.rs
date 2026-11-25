//! Checked arithmetic for the renderer and the app shell: the small helpers
//! every screen-geometry calculation goes through, so no `+`, `-`, `*` or
//! `[..]` on a screen offset can overflow or panic (`rust-checked-arithmetic`).
//!
//! Each helper names its declared fallback, the correct answer to a failed
//! operation on valid values: a sum past the top of a type is the top (off
//! screen), a difference below zero is zero (no room left), and a rectangle the
//! layout did not produce is the empty rectangle (nothing is drawn there).
//! Owning spec: `tui/spec/index.md`.

use ratatui::layout::Rect;

/// Checked `+`, `-`, `*`, `/` and `%` on screen and text offsets: a sum or
/// product past the top of the type is the top, and a difference below zero is
/// zero. For signed types an overflow goes to the nearer end of the type.
pub(crate) trait Geo: Sized {
    /// `self + other`, or the type's top when it does not fit.
    fn plus(self, other: Self) -> Self;
    /// `self - other`, or zero when it would go below zero.
    fn minus(self, other: Self) -> Self;
    /// `self * other`, or the type's top when it does not fit.
    fn times(self, other: Self) -> Self;
    /// `self / other` rounded down, or zero when `other` is zero (dividing by
    /// nothing leaves no room).
    fn quot(self, other: Self) -> Self;
    /// `self % other`, or zero when `other` is zero.
    fn modulo(self, other: Self) -> Self;
}

macro_rules! geo_unsigned {
    ($($t:ty),*) => {$(
        impl Geo for $t {
            fn plus(self, other: Self) -> Self {
                self.checked_add(other).unwrap_or(Self::MAX)
            }
            fn minus(self, other: Self) -> Self {
                self.checked_sub(other).unwrap_or(0)
            }
            fn times(self, other: Self) -> Self {
                self.checked_mul(other).unwrap_or(Self::MAX)
            }
            fn quot(self, other: Self) -> Self {
                self.checked_div(other).unwrap_or(0)
            }
            fn modulo(self, other: Self) -> Self {
                self.checked_rem(other).unwrap_or(0)
            }
        }
    )*};
}

geo_unsigned!(u8, u16, u32, u64, u128, usize);

macro_rules! geo_signed {
    ($($t:ty),*) => {$(
        impl Geo for $t {
            fn plus(self, other: Self) -> Self {
                self.checked_add(other)
                    .unwrap_or(if other > 0 { Self::MAX } else { Self::MIN })
            }
            fn minus(self, other: Self) -> Self {
                self.checked_sub(other)
                    .unwrap_or(if other > 0 { Self::MIN } else { Self::MAX })
            }
            fn times(self, other: Self) -> Self {
                self.checked_mul(other).unwrap_or(
                    if (self < 0) == (other < 0) { Self::MAX } else { Self::MIN },
                )
            }
            fn quot(self, other: Self) -> Self {
                self.checked_div(other).unwrap_or(0)
            }
            fn modulo(self, other: Self) -> Self {
                self.checked_rem(other).unwrap_or(0)
            }
        }
    )*};
}

geo_signed!(i32, i64, isize);

/// `now + delay`, or `now` when the sum cannot be represented (a debounce
/// delay is a small constant, so the sum always fits).
pub(crate) fn later(now: std::time::Instant, delay: std::time::Duration) -> std::time::Instant {
    now.checked_add(delay).unwrap_or(now)
}

/// Rectangle `i` of a layout split, or the empty rectangle when the split has
/// fewer: a layout produces exactly one rectangle per constraint, so the empty
/// one is never drawn into.
pub(crate) fn chunk(chunks: &[Rect], i: usize) -> Rect {
    chunks.get(i).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_minus_times_stop_at_the_edges() {
        assert_eq!(u16::MAX.plus(1), u16::MAX);
        assert_eq!(3u16.minus(5), 0);
        assert_eq!(200u8.times(2), u8::MAX);
        assert_eq!(2usize.plus(3).minus(1).times(4), 16);
        assert_eq!(7u16.quot(2), 3);
        assert_eq!(7u16.modulo(0), 0);
        assert_eq!(7u16.quot(0), 0);
    }

    #[test]
    fn a_missing_layout_rectangle_is_empty() {
        let one = [Rect::new(1, 2, 3, 4)];
        assert_eq!(chunk(&one, 0), Rect::new(1, 2, 3, 4));
        assert_eq!(chunk(&one, 1), Rect::default());
    }
}
