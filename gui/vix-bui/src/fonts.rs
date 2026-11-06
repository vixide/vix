//! The window's fonts: the monospace family chosen in `crate::font_choice`,
//! found by name through `fontique`, measured with `ttf-parser`, and handed
//! to BUI as bytes.
//!
//! BUI draws a canvas's text in its *default* family, with no way to name
//! another one from `Painter::text`, but `blenderui::add_font` under the empty
//! family name replaces that default for the whole process. So ordinary text
//! stays on the canvas, whose cost per character is the lowest BUI has, and
//! only text in another face goes through elements with a `font-family`
//! (bold, italic, and characters the primary font lacks). BUI itself has no
//! per-glyph fallback, so [`Fonts::route`] does that here. With no font found
//! at all, BUI's embedded Inter remains, with its own cell metrics.
//!
//! BUI opens a font's *first* face and takes no collection index, while a
//! style of a `.ttc` may be any of its faces (Menlo's bold is one):
//! [`standalone`] makes the wanted face the first.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use fontique::{
    Blob, Collection, CollectionOptions, FontStyle, FontWeight, FontWidth, SourceCache,
};

use crate::font_choice::{self, FONT_SIZE};

/// The element `font-family` of the primary font's faces.
pub const MONO: &str = "vix-mono";

/// At most this many fallback families are tried. BUI holds 32 faces in all.
const MAX_FALLBACKS: usize = 16;

/// A face at least this heavy counts as a real bold (below it, a family's
/// "bold" match is its regular, and BUI should synthesize instead).
const BOLD_WEIGHT: f32 = 600.0;

/// A font's size-dependent numbers, in logical pixels at
/// [`FONT_SIZE`](crate::font_choice::FONT_SIZE).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellFont {
    /// The advance of one character.
    pub advance: f32,
    /// Height above the baseline.
    pub ascent: f32,
    /// Depth below the baseline, positive.
    pub descent: f32,
}

impl CellFont {
    /// The numbers of the first face in `data`.
    fn parse(data: &[u8]) -> Option<Self> {
        let face = ttf_parser::Face::parse(data, 0).ok()?;
        let scale = FONT_SIZE / f32::from(face.units_per_em());
        let advance = face.glyph_hor_advance(face.glyph_index('M')?)?;
        Some(Self {
            advance: f32::from(advance) * scale,
            ascent: f32::from(face.ascender()) * scale,
            descent: f32::from(face.descender()).abs() * scale,
        })
    }
}

/// `data` (holding the face at `index`) as a font whose first face is that
/// one, or `None` when it cannot be. BUI always opens the first; in a
/// collection (`ttcf`) the first face is simply whichever the first entry of
/// the offset table at byte 12 points to, so one entry is overwritten.
fn standalone(data: &[u8], index: u32) -> Option<Vec<u8>> {
    if index == 0 {
        return Some(data.to_vec());
    }
    if !data.starts_with(b"ttcf") {
        return None;
    }
    let entry = 12 + 4 * usize::try_from(index).ok()?;
    let offset = data.get(entry..entry + 4)?.to_vec();
    let mut out = data.to_vec();
    out.get_mut(12..16)?.copy_from_slice(&offset);
    Some(out)
}

/// Whether the face at `index` of `data` draws `ch` as an outline. BUI asks
/// its font engine for outlines only, so a bitmap color font's emoji would come out
/// empty: such a font does not count as having them.
fn covers(data: &[u8], index: u32, ch: char) -> bool {
    struct Discard;
    impl ttf_parser::OutlineBuilder for Discard {
        fn move_to(&mut self, _: f32, _: f32) {}
        fn line_to(&mut self, _: f32, _: f32) {}
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
        fn close(&mut self) {}
    }
    let Ok(face) = ttf_parser::Face::parse(data, index) else {
        return false;
    };
    face.glyph_index(ch)
        .is_some_and(|glyph| face.outline_glyph(glyph, &mut Discard).is_some())
}

/// Register `data` with BUI under `family` once in this process: the faces
/// BUI holds are global, limited, and never released. Must run inside
/// `Application::run`, which holds BUI's lock. `data` is called only when
/// needed.
fn add_once(family: &str, bold: bool, italic: bool, data: impl FnOnce() -> Option<Vec<u8>>) {
    static DONE: Mutex<Option<HashSet<(String, bool, bool)>>> = Mutex::new(None);
    let Ok(mut done) = DONE.lock() else {
        return;
    };
    let key = (family.to_string(), bold, italic);
    if done.get_or_insert_default().contains(&key) {
        return;
    }
    if let Some(bytes) = data()
        && blenderui::add_font(family, bold, italic, &bytes)
        && let Some(set) = done.as_mut()
    {
        set.insert(key);
    }
}

/// A fallback family's font file.
enum Opened {
    /// No character has needed it yet.
    NotYet,
    /// It could not be read.
    Failed,
    /// The file's bytes and the face's index.
    Font(Blob<u8>, u32),
}

/// One fallback family, opened the first time a character needs it.
struct Fallback {
    name: String,
    font: Opened,
}

/// A face of the primary family other than its regular.
struct Styled {
    bold: bool,
    italic: bool,
    font: (Blob<u8>, u32),
}

/// The primary font and what falls back from it.
struct Primary {
    regular: (Blob<u8>, u32),
    styled: Vec<Styled>,
    metrics: CellFont,
    collection: Collection,
    cache: SourceCache,
    fallbacks: Vec<Fallback>,
    /// What [`Fonts::route`] decided for each character so far.
    routes: HashMap<char, Option<u8>>,
}

/// One face of `family`, as the file's bytes and the face's index.
fn face(
    collection: &mut Collection,
    cache: &mut SourceCache,
    family: &str,
    style: FontStyle,
    weight: FontWeight,
) -> Option<(fontique::FontInfo, (Blob<u8>, u32))> {
    let info = collection.family_by_name(family)?;
    let index = info.match_index(FontWidth::NORMAL, style, weight, false)?;
    let font = info.fonts().get(index)?;
    let blob = font.load(Some(cache))?;
    Some((font.clone(), (blob, font.index())))
}

/// The real bold and italic faces of `family`; a style it lacks is left out,
/// and BUI synthesizes it from the regular face.
fn styled_faces(collection: &mut Collection, cache: &mut SourceCache, family: &str) -> Vec<Styled> {
    let mut out = Vec::new();
    for (bold, italic) in [(true, false), (false, true), (true, true)] {
        let weight = if bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };
        let style = if italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        if let Some((info, font)) = face(collection, cache, family, style, weight)
            && (!bold || info.weight().value() >= BOLD_WEIGHT)
            && (!italic || info.style() != FontStyle::Normal)
        {
            out.push(Styled { bold, italic, font });
        }
    }
    out
}

/// The fonts of one window.
#[derive(Default)]
pub struct Fonts {
    /// `None`: no font was found, so BUI's embedded Inter draws.
    primary: Option<Primary>,
}

impl Fonts {
    /// BUI's embedded font alone, for a machine with no usable font.
    #[cfg(test)]
    #[must_use]
    pub fn embedded() -> Self {
        Self::default()
    }

    /// Find the primary family on this machine, or fall back to [`Self::default`]
    /// when it is not installed, cannot be read or has no usable metrics.
    #[must_use]
    pub fn load() -> Self {
        Self {
            primary: Self::find(&font_choice::primary_family()),
        }
    }

    /// The fonts for a monospace `family`, if it is installed.
    fn find(family: &str) -> Option<Primary> {
        let mut collection = Collection::new(CollectionOptions::default());
        let mut cache = SourceCache::default();
        let (_, regular) = face(
            &mut collection,
            &mut cache,
            family,
            FontStyle::Normal,
            FontWeight::NORMAL,
        )?;
        let bytes = standalone(regular.0.as_ref(), regular.1)?;
        let metrics = CellFont::parse(&bytes)?;
        let styled = styled_faces(&mut collection, &mut cache, family);
        let installed: Vec<String> = collection.family_names().map(String::from).collect();
        let fallbacks = font_choice::fallbacks(&installed)
            .into_iter()
            .take(MAX_FALLBACKS)
            .map(|name| Fallback {
                name,
                font: Opened::NotYet,
            })
            .collect();
        Some(Primary {
            regular,
            styled,
            metrics,
            collection,
            cache,
            fallbacks,
            routes: HashMap::new(),
        })
    }

    /// The primary font's numbers, if there is one.
    #[must_use]
    pub fn metrics(&self) -> Option<CellFont> {
        self.primary.as_ref().map(|p| p.metrics)
    }

    /// Give BUI the primary font, as its default family (what the canvas
    /// draws in) and as [`MONO`]. Call inside `Application::run`.
    pub fn install(&self) {
        let Some(p) = &self.primary else {
            return;
        };
        let (blob, index) = &p.regular;
        let regular = || standalone(blob.as_ref(), *index);
        add_once("", false, false, regular);
        add_once(MONO, false, false, regular);
        for s in &p.styled {
            let (blob, index) = &s.font;
            add_once(MONO, s.bold, s.italic, || standalone(blob.as_ref(), *index));
        }
    }

    /// Which fallback family draws `ch`: `None` for the primary font (which
    /// is also what shows a missing-glyph box when nothing has it). A family
    /// is given to BUI the first time it is chosen, so this runs inside
    /// `Application::run`, like [`Self::install`].
    pub fn route(&mut self, ch: char) -> Option<u8> {
        let p = self.primary.as_mut()?;
        if let Some(route) = p.routes.get(&ch) {
            return *route;
        }
        let route = p.fallback_for(ch);
        p.routes.insert(ch, route);
        route
    }
}

impl Primary {
    /// The first fallback that has `ch`, if the primary font does not.
    fn fallback_for(&mut self, ch: char) -> Option<u8> {
        if covers(self.regular.0.as_ref(), self.regular.1, ch) {
            return None;
        }
        for i in 0..self.fallbacks.len() {
            let fallback = &mut self.fallbacks[i];
            if matches!(fallback.font, Opened::NotYet) {
                fallback.font = face(
                    &mut self.collection,
                    &mut self.cache,
                    &fallback.name,
                    FontStyle::Normal,
                    FontWeight::NORMAL,
                )
                .map_or(Opened::Failed, |(_, (blob, index))| {
                    Opened::Font(blob, index)
                });
            }
            let Opened::Font(blob, index) = &fallback.font else {
                continue;
            };
            if covers(blob.as_ref(), *index, ch) {
                let id = u8::try_from(i).ok()?;
                add_once(&fallback_family(id), false, false, || {
                    standalone(blob.as_ref(), *index)
                });
                return Some(id);
            }
        }
        None
    }
}

/// The element `font-family` of fallback number `id`.
#[must_use]
pub fn fallback_family(id: u8) -> String {
    format!("vix-fallback-{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-face collection header followed by filler.
    fn collection() -> Vec<u8> {
        let mut data = b"ttcf\0\x01\0\0\0\0\0\x02".to_vec();
        data.extend_from_slice(&[0, 0, 0, 20]);
        data.extend_from_slice(&[0, 0, 0, 40]);
        data.extend_from_slice(&[7; 8]);
        data
    }

    #[test]
    fn the_first_face_of_a_font_is_itself() {
        assert_eq!(
            standalone(b"plain font", 0).as_deref(),
            Some(&b"plain font"[..])
        );
    }

    #[test]
    fn a_later_face_of_a_collection_becomes_the_first() {
        let out = standalone(&collection(), 1).expect("a collection");
        assert_eq!(out[12..16], [0, 0, 0, 40]);
        assert_eq!(out[16..20], [0, 0, 0, 40], "the rest is untouched");
        assert_eq!(out.len(), collection().len());
    }

    #[test]
    fn a_face_that_cannot_be_first_is_refused() {
        assert_eq!(standalone(b"not a collection", 1), None);
        assert_eq!(standalone(&collection()[..14], 1), None);
        assert_eq!(standalone(b"ttcf", 9), None);
    }

    #[test]
    fn garbage_covers_nothing_and_has_no_metrics() {
        assert!(!covers(b"garbage", 0, 'a'));
        assert_eq!(CellFont::parse(b"garbage"), None);
    }

    #[test]
    fn with_no_font_everything_is_the_embedded_default() {
        let mut fonts = Fonts::embedded();
        assert_eq!(fonts.metrics(), None);
        assert_eq!(fonts.route('\u{f07b}'), None);
    }

    #[test]
    fn fallback_families_are_named_by_number() {
        assert_eq!(fallback_family(3), "vix-fallback-3");
    }
}
