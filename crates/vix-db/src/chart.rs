//! A tiny horizontal bar chart for two-column query results.
//!
//! [`bars`] turns a `(label, …, number)` result — the first column labels each
//! bar, the last numeric-parseable column sizes it — into an ASCII chart shown
//! in the workbench's text viewer. Pure and unit-tested; the workbench only
//! feeds it the current (filtered) grid.

use std::fmt::Write as _;

/// Full width, in cells, of the longest bar.
const WIDTH: usize = 40;

/// The widest label kept before truncating with an ellipsis.
const LABEL_MAX: usize = 24;

/// Render `rows` as a horizontal bar chart, labelling each bar with the first
/// column and sizing it by the last column parsed as a number. Returns `None`
/// when there is no data, fewer than two columns, or no parseable numbers.
#[must_use]
pub fn bars(headers: &[String], rows: &[&Vec<String>]) -> Option<String> {
    if headers.len() < 2 || rows.is_empty() {
        return None;
    }
    let value_col = headers.len() - 1;
    let parsed: Vec<(String, f64)> = rows
        .iter()
        .filter_map(|r| {
            let label = r.first().cloned().unwrap_or_default();
            let value = r.get(value_col)?.trim().parse::<f64>().ok()?;
            Some((label, value))
        })
        .collect();
    if parsed.is_empty() {
        return None;
    }
    let peak = parsed.iter().fold(0f64, |m, (_, v)| m.max(v.abs()));
    let label_w = parsed
        .iter()
        .map(|(l, _)| l.chars().count().min(LABEL_MAX))
        .max()
        .unwrap_or(0);

    let mut out = format!("{}  by  {}\n\n", headers[0], headers[value_col]);
    for (label, value) in &parsed {
        let ratio = if peak > 0.0 { value.abs() / peak } else { 0.0 };
        let bar = bar_of(ratio);
        let label = truncate(label, LABEL_MAX);
        let _ = writeln!(out, "{label:>label_w$} │{bar} {}", trim_number(*value));
    }
    Some(out)
}

/// A bar of `ratio` (0.0–1.0) of the full [`WIDTH`], built cell by cell so no
/// float is ever cast to an integer.
fn bar_of(ratio: f64) -> String {
    let width_f = f64::from(u32::try_from(WIDTH).unwrap_or(u32::MAX));
    let mut bar = String::new();
    for i in 0..WIDTH {
        let cell = f64::from(u32::try_from(i).unwrap_or(u32::MAX));
        if cell + 0.5 <= ratio * width_f {
            bar.push('█');
        }
    }
    bar
}

/// Truncate `label` to `max` chars with a trailing `…` when it overflows.
fn truncate(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        return label.to_string();
    }
    let kept: String = label.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Format a value without a trailing `.0` on whole numbers.
fn trim_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(data: &[(&str, &str)]) -> Vec<Vec<String>> {
        data.iter()
            .map(|(a, b)| vec![(*a).to_string(), (*b).to_string()])
            .collect()
    }

    #[test]
    fn renders_bars_scaled_to_the_peak() {
        let owned = rows(&[("ada", "10"), ("grace", "5")]);
        let refs: Vec<&Vec<String>> = owned.iter().collect();
        let out = bars(&["name".into(), "n".into()], &refs).unwrap();
        assert!(out.contains("name  by  n"), "{out}");
        // ada (10) is the peak → full-width bar; grace (5) → half.
        let ada = out.lines().find(|l| l.contains("ada")).unwrap();
        let grace = out.lines().find(|l| l.contains("grace")).unwrap();
        assert_eq!(ada.matches('█').count(), WIDTH);
        assert_eq!(grace.matches('█').count(), WIDTH / 2);
        assert!(ada.trim_end().ends_with("10"));
    }

    #[test]
    fn rejects_non_numeric_and_empty() {
        let owned = rows(&[("a", "x"), ("b", "y")]);
        let refs: Vec<&Vec<String>> = owned.iter().collect();
        assert!(
            bars(&["k".into(), "v".into()], &refs).is_none(),
            "no numbers"
        );
        assert!(
            bars(&["only".into()], &[]).is_none(),
            "one column / no rows"
        );
    }

    #[test]
    fn truncates_long_labels() {
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(truncate("short", 10), "short");
    }
}
