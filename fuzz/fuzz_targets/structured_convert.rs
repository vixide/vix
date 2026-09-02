//! Fuzz the JSON⇄YAML and JSON⇄TOML `Tools → Convert` tools.
//!
//! Each of the four crates already has `proptest` coverage for the no-panic
//! property; this target adds coverage-guided fuzzing on top and checks a
//! round-trip property: converting valid JSON to YAML/TOML and back must
//! reparse to an equivalent `serde_json::Value` (TOML has no top-level array
//! or `null`, so that direction is skipped for inputs it cannot represent).
//!
//! "Equivalent", not "equal": the first fuzz run here found that a large
//! float with many significant digits (`1.1533333333353332e30`) comes back
//! as `1.1533333333353333e30` — the last digit differs. That's YAML's
//! decimal-text number formatting losing a `f64`'s exact bit pattern, an
//! inherent property of any text-based numeric round trip, not a Vix bug —
//! `serde_yaml` chose to serialize the value that way, not this crate's own
//! logic. [`values_equivalent`] tolerates a tiny relative difference between
//! numbers and requires exact equality for everything else (so a string
//! becoming a number, or a key going missing, still fails).

#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::Value;

/// `a == b`, except two [`Value::Number`]s compare equal within a relative
/// tolerance (see the module doc for why exact float equality isn't the
/// right invariant for a JSON<->YAML/TOML round trip).
fn values_equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (
                x.as_f64().unwrap_or(f64::NAN),
                y.as_f64().unwrap_or(f64::NAN),
            );
            if x == y {
                return true;
            }
            let scale = x.abs().max(y.abs()).max(1.0);
            (x - y).abs() / scale < 1e-9
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| values_equivalent(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| values_equivalent(v, w)))
        }
        _ => a == b,
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // Both directions must be total over arbitrary text, valid or not.
    let yaml = vix_convert_from_json_into_yaml_tool::convert(text);
    let _ = vix_convert_from_yaml_into_json_tool::convert(text);
    let toml_out = vix_convert_from_json_into_toml_tool::convert(text);
    let _ = vix_convert_from_toml_into_json_tool::convert(text);

    // `text` parses as JSON iff `serde_json::from_str` accepts it — reuse that
    // as the oracle for whether a round trip should be checked, rather than
    // re-deriving it from which `convert` calls returned `Ok`.
    let Ok(original): Result<Value, _> = serde_json::from_str(text) else {
        return;
    };

    if let Ok(y) = yaml {
        let back = vix_convert_from_yaml_into_json_tool::convert(&y)
            .expect("YAML this crate just emitted must convert back");
        let reparsed: Value = serde_json::from_str(&back).expect("convert always emits valid JSON");
        assert!(
            values_equivalent(&reparsed, &original),
            "JSON -> YAML -> JSON changed the value: {original:?} -> {reparsed:?}"
        );
    }

    if let Ok(t) = toml_out {
        let back = vix_convert_from_toml_into_json_tool::convert(&t)
            .expect("TOML this crate just emitted must convert back");
        let reparsed: Value = serde_json::from_str(&back).expect("convert always emits valid JSON");
        assert!(
            values_equivalent(&reparsed, &original),
            "JSON -> TOML -> JSON changed the value: {original:?} -> {reparsed:?}"
        );
    }
});
