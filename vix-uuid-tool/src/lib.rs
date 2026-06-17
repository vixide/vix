//! Generate UUIDs of every version, v1 through v8.
//!
//! Vix's Tools → Generate → UUID submenu offers one item per version; each calls
//! the matching function here and inserts the hyphenated, lowercase result at the
//! cursor. The versions differ in how the 128 bits are derived:
//!
//! - **v1** — timestamp + monotonic counter + node id (a random node id stands in
//!   for the MAC address, with the multicast bit set per RFC 4122 §4.5).
//! - **v2** — DCE Security: a v1 layout whose low time field carries a local
//!   domain id and whose `clock_seq_low` carries the domain (Person = 0).
//! - **v3** — deterministic: MD5 of a namespace + name.
//! - **v4** — random.
//! - **v5** — deterministic: SHA-1 of a namespace + name.
//! - **v6** — like v1 but field-ordered so the IDs sort by creation time.
//! - **v7** — Unix-epoch milliseconds + random data (sortable, modern default).
//! - **v8** — the all-zero nil UUID (Vix's chosen v8 payload).
//!
//! v3 and v5 are deterministic given a (namespace, name) pair. Since the menu
//! items take no input, [`v3`] and [`v5`] hash a fresh random name under the URL
//! namespace, so each invocation yields a distinct — but correctly constructed —
//! identifier. The deterministic core is [`v3_named`] / [`v5_named`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use uuid::Uuid;

/// Six random bytes for use as a v1/v6 node id, with the multicast bit set so it
/// can never be mistaken for a real network MAC address (RFC 4122 §4.5).
fn random_node() -> [u8; 6] {
    let bytes = Uuid::new_v4().into_bytes();
    let mut node = [0u8; 6];
    node.copy_from_slice(&bytes[..6]);
    node[0] |= 0x01;
    node
}

/// UUID v1: timestamp + monotonic counter + (random) node id.
#[must_use]
pub fn v1() -> String {
    Uuid::now_v1(&random_node()).hyphenated().to_string()
}

/// UUID v2 (DCE Security): a v1 layout carrying a local domain id and a domain
/// number. Vix uses domain Person (0) with a zero id, since the menu takes no
/// input; the version and variant bits are set correctly so the value is a valid
/// v2 UUID.
#[must_use]
pub fn v2() -> String {
    let mut bytes = Uuid::now_v1(&random_node()).into_bytes();
    // Low time field becomes the local domain id (Person id = 0).
    bytes[0..4].copy_from_slice(&0u32.to_be_bytes());
    // Set the version nibble to 2.
    bytes[6] = (bytes[6] & 0x0f) | 0x20;
    // clock_seq_low carries the DCE local domain (Person = 0).
    bytes[9] = 0;
    Uuid::from_bytes(bytes).hyphenated().to_string()
}

/// UUID v3: MD5 hash of the URL namespace and a fresh random name.
#[must_use]
pub fn v3() -> String {
    v3_named(&Uuid::new_v4().to_string())
}

/// UUID v3 from an explicit name under the URL namespace (deterministic).
#[must_use]
pub fn v3_named(name: &str) -> String {
    Uuid::new_v3(&Uuid::NAMESPACE_URL, name.as_bytes()).hyphenated().to_string()
}

/// UUID v4: 122 random bits.
#[must_use]
pub fn v4() -> String {
    Uuid::new_v4().hyphenated().to_string()
}

/// UUID v5: SHA-1 hash of the URL namespace and a fresh random name.
#[must_use]
pub fn v5() -> String {
    v5_named(&Uuid::new_v4().to_string())
}

/// UUID v5 from an explicit name under the URL namespace (deterministic).
#[must_use]
pub fn v5_named(name: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, name.as_bytes()).hyphenated().to_string()
}

/// UUID v6: like v1 but time-ordered so the IDs are lexically sortable.
#[must_use]
pub fn v6() -> String {
    Uuid::now_v6(&random_node()).hyphenated().to_string()
}

/// UUID v7: Unix-epoch milliseconds combined with random data (sortable).
#[must_use]
pub fn v7() -> String {
    Uuid::now_v7().hyphenated().to_string()
}

/// UUID v8: the all-zero nil UUID, `00000000-0000-0000-0000-000000000000`.
#[must_use]
pub fn v8() -> String {
    Uuid::nil().hyphenated().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version nibble is the first hex digit of the third group.
    fn version_digit(uuid: &str) -> char {
        uuid.chars().nth(14).expect("version position")
    }

    /// The variant digit is the first hex digit of the fourth group.
    fn variant_digit(uuid: &str) -> char {
        uuid.chars().nth(19).expect("variant position")
    }

    fn is_canonical(uuid: &str) -> bool {
        uuid.len() == 36
            && uuid.as_bytes().iter().enumerate().all(|(i, &b)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
                }
            })
    }

    #[test]
    fn all_versions_are_canonical_lowercase() {
        for u in [v1(), v2(), v3(), v4(), v5(), v6(), v7(), v8()] {
            assert!(is_canonical(&u), "not canonical lowercase: {u}");
        }
    }

    #[test]
    fn version_digits_match() {
        assert_eq!(version_digit(&v1()), '1');
        assert_eq!(version_digit(&v2()), '2');
        assert_eq!(version_digit(&v3()), '3');
        assert_eq!(version_digit(&v4()), '4');
        assert_eq!(version_digit(&v5()), '5');
        assert_eq!(version_digit(&v6()), '6');
        assert_eq!(version_digit(&v7()), '7');
    }

    #[test]
    fn rfc_variant_bits_set() {
        // RFC 4122 variant: the high bits make the digit one of 8, 9, a, b.
        for u in [v1(), v2(), v3(), v4(), v5(), v6(), v7()] {
            assert!(matches!(variant_digit(&u), '8' | '9' | 'a' | 'b'), "bad variant: {u}");
        }
    }

    #[test]
    fn v3_and_v5_are_deterministic() {
        assert_eq!(v3_named("vix"), v3_named("vix"));
        assert_eq!(v5_named("vix"), v5_named("vix"));
        assert_ne!(v3_named("vix"), v3_named("vox"));
        assert_eq!(version_digit(&v3_named("vix")), '3');
        assert_eq!(version_digit(&v5_named("vix")), '5');
    }

    #[test]
    fn v8_is_nil() {
        assert_eq!(v8(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn random_versions_differ_each_call() {
        assert_ne!(v4(), v4());
        assert_ne!(v7(), v7());
    }
}
