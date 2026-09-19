//! Format a byte count as a human-readable size (`16.0 KiB`).
//!
//! Extracted (Run H, T521) after `vix-file-information-panel` and
//! `vix-system-information-panel` were found to each hand-roll an identical
//! copy of this function, including its doc comment — a real duplication,
//! not domain-driven similarity, since both crates format the exact same
//! quantity (a file or disk size in bytes) the exact same way.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert a `u64` to the nearest `f64` without a lossy `as` cast. The high
/// and low 32-bit halves are each representable exactly, so the single
/// rounding on the recombining add matches what `n as f64` would produce.
fn u64_to_f64(n: u64) -> f64 {
    let high = u32::try_from(n >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(n & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    f64::from(high) * 4_294_967_296.0 + f64::from(low)
}

/// Format a byte count as a human-readable size (`16.0 KiB`).
#[must_use]
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = u64_to_f64(n);
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_1024_bytes_is_shown_plainly() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
    }

    #[test]
    fn scales_up_through_the_unit_table() {
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(16 * 1024), "16.0 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn caps_at_the_largest_unit_instead_of_indexing_past_it() {
        // Larger than any real file, but must still format instead of
        // panicking on an out-of-bounds `UNITS` index.
        assert_eq!(human_bytes(u64::MAX), "16384.0 PiB");
    }

    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "the whole point of this test is exact equality with a lossy `as` cast at \
                  sizes where the cast isn't actually lossy (below 2^53) -- an epsilon \
                  comparison would defeat that"
    )]
    fn u64_to_f64_matches_a_lossy_as_cast_at_realistic_sizes() {
        // `as` casting is lossy only above 2^53; every size this crate
        // actually formats (real file/disk sizes) is far below that, so the
        // two must agree exactly there.
        for n in [0_u64, 1, 1024, u64::from(u32::MAX), 5_000_000_000] {
            #[allow(clippy::cast_precision_loss)]
            let expected = n as f64;
            assert_eq!(u64_to_f64(n), expected, "mismatch for {n}");
        }
    }
}
