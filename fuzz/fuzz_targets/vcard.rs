//! Fuzz the vCard (RFC 6350) parser.
//!
//! Vix parses `.vcf` files a user opens — third-party data from a phone, a mail
//! client, or an export — so the parser must survive folded lines, unknown
//! parameters, missing `BEGIN`/`END`, and stray bytes without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let card = vix_vcard_parser::parse(text);

    // Reading the card must be as total as parsing it.
    let _ = card.display_name();
    let _ = card.value("FN");
    let _ = card.value("EMAIL");
    let _ = card.all("TEL");
    if let Some(prop) = card.get("ADR") {
        let _ = prop.param("TYPE");
        let _ = prop.types();
    }
});
