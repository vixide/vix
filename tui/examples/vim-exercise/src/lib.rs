//! The Vim migration exercise (`docs/vix-for-vim-users/index.md`): two
//! functions pass the same wrong argument, and a test catches it.

/// How many percent `part` is of `whole`, rounded down.
pub fn percent(part: u32, whole: u32) -> u32 {
    ratio(part, whole, 10)
}

/// The same percentage as text, such as `25%`.
pub fn percent_label(part: u32, whole: u32) -> String {
    format!("{}%", ratio(part, whole, 10))
}

fn ratio(part: u32, whole: u32, scale: u32) -> u32 {
    part * scale / whole
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_is_25_percent() {
        assert_eq!(percent(1, 4), 25);
    }

    #[test]
    fn the_label_ends_in_a_percent_sign() {
        assert_eq!(percent_label(1, 2), "50%");
    }
}
