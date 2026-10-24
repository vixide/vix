//! Load the window's fonts into egui: the primary monospace family and Nerd
//! Font fallbacks chosen in `crate::font_choice`, found by
//! family name through `fontique` — the OS font database egui's own system
//! fallback is built on. egui keeps its bundled fonts as the last fallback.

use std::sync::Arc;

use crate::font_choice as fonts;
use eframe::egui::{FontData, FontDefinitions, FontFamily};
use fontique::{Collection, CollectionOptions, FontStyle, FontWeight, FontWidth, SourceCache};

/// The egui family name used for bold text.
pub const BOLD: &str = "vix-bold";

/// Read one face of an installed family (by weight), or `None` if absent.
fn load(
    coll: &mut Collection,
    cache: &mut SourceCache,
    family: &str,
    weight: FontWeight,
) -> Option<FontData> {
    let fam = coll.family_by_name(family)?;
    let idx = fam.match_index(FontWidth::NORMAL, FontStyle::Normal, weight, false)?;
    let font = fam.fonts().get(idx)?;
    // fontique's blob is the memory-mapped file; share it, don't copy it.
    let blob = font.load(Some(cache))?;
    Some(FontData::from_blob(Arc::new(blob), font.index()))
}

/// egui's font definitions for the window: `Monospace` (regular) and
/// [`BOLD`], each the primary family, then Nerd Font fallbacks, then egui's
/// bundled monospace fonts.
#[must_use]
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let mut coll = Collection::new(CollectionOptions::default());
    let mut cache = SourceCache::default();
    let installed: Vec<String> = coll.family_names().map(String::from).collect();
    let primary = fonts::primary_family();

    let mut regular = Vec::new();
    let mut bold = Vec::new();
    if let Some(d) = load(&mut coll, &mut cache, &primary, FontWeight::NORMAL) {
        defs.font_data.insert("vix-regular".into(), Arc::new(d));
        regular.push("vix-regular".to_string());
    }
    if let Some(d) = load(&mut coll, &mut cache, &primary, FontWeight::BOLD) {
        defs.font_data.insert("vix-bold".into(), Arc::new(d));
        bold.push("vix-bold".to_string());
    }
    for (i, family) in fonts::nerd_fallbacks(&installed).iter().enumerate() {
        if let Some(d) = load(&mut coll, &mut cache, family, FontWeight::NORMAL) {
            let name = format!("vix-nerd-{i}");
            defs.font_data.insert(name.clone(), Arc::new(d));
            regular.push(name.clone());
            bold.push(name);
        }
    }
    let bundled = defs
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    regular.extend(bundled.iter().cloned());
    bold.extend(bundled);
    defs.families.insert(FontFamily::Monospace, regular);
    defs.families.insert(FontFamily::Name(BOLD.into()), bold);
    defs
}
