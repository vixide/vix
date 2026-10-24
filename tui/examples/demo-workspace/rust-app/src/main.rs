//! A tiny word-frequency counter — the Rust half of the demo workspace's
//! tour. Deliberately unfinished in two places (see TODO/FIXME below) so
//! the tutorials have something real to fix.

use std::collections::HashMap;

fn main() {
    let text = "the quick brown fox jumps over the lazy dog the fox runs";
    let counts = word_counts(text);

    let mut pairs: Vec<_> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    for (word, count) in &pairs {
        println!("{word}: {count}");
    }

    // TODO: report the total word count too, not just the per-word breakdown.
    // FIXME: word_counts lowercases nothing, so "The" and "the" count separately.
}

fn word_counts(text: &str) -> HashMap<&str, u32> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_repeated_words() {
        let counts = word_counts("a b a c a b");
        assert_eq!(counts.get("a"), Some(&3));
        assert_eq!(counts.get("b"), Some(&2));
        assert_eq!(counts.get("c"), Some(&1));
    }
}
