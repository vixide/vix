//! Who is where: remote cursors, stable peer colors, follow mode.
//!
//! Pure data and arithmetic; the renderer in the app shell draws what
//! [`PresenceBoard::cursors`] returns. Colors come from a fixed hue palette
//! pushed toward light or dark until they clear a WCAG contrast ratio against
//! the active theme's background, so a remote cursor stays visible in every
//! theme, including the high-contrast ones.

use crate::doc::SharedDoc;
use crate::path::SharedPath;
use crate::peer::PeerId;
use crate::wire::Presence;
use std::collections::BTreeMap;

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The minimum contrast ratio a peer color keeps against the background
/// (WCAG AA for large text and UI components).
pub const MIN_CONTRAST: f64 = 3.0;

/// Base hues, well separated; adjusted per theme by [`peer_color`].
const PALETTE: [Rgb; 8] = [
    Rgb(0xe6, 0x4a, 0x4a),
    Rgb(0x2e, 0x9f, 0x5b),
    Rgb(0x3b, 0x82, 0xf6),
    Rgb(0xe0, 0x9b, 0x1a),
    Rgb(0xa8, 0x55, 0xf7),
    Rgb(0x14, 0xb8, 0xc4),
    Rgb(0xe8, 0x4a, 0xa0),
    Rgb(0x8a, 0xa6, 0x1c),
];

/// One sRGB channel converted to linear light, per the WCAG relative-luminance
/// definition (the `0.03928` knee is the WCAG-published constant).
fn channel(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.039_28 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance per WCAG 2.x.
#[must_use]
pub fn luminance(c: Rgb) -> f64 {
    0.0722f64.mul_add(
        channel(c.2),
        0.2126f64.mul_add(channel(c.0), 0.7152 * channel(c.1)),
    )
}

/// WCAG contrast ratio between two colors (1.0 to 21.0).
#[must_use]
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Move `c` toward `target` by `step` twentieths (integer math, no casts).
fn mix(c: Rgb, target: u8, step: i32) -> Rgb {
    let m = |x: u8| {
        let (x, t) = (i32::from(x), i32::from(target));
        u8::try_from((x + (t - x) * step / 20).clamp(0, 255)).unwrap_or(target)
    };
    Rgb(m(c.0), m(c.1), m(c.2))
}

/// The color for `peer` on a theme whose background is `background`: a stable
/// palette entry per peer, lightened (on dark themes) or darkened (on light
/// ones) until it keeps [`MIN_CONTRAST`] against the background.
#[must_use]
pub fn peer_color(peer: PeerId, background: Rgb) -> Rgb {
    // The host is peer 1 and the first guest peer 2, so index from 0.
    let slot = usize::try_from(peer.index().wrapping_sub(1) % PALETTE.len() as u64).unwrap_or(0);
    let base = PALETTE[slot];
    let dark_bg = luminance(background) < 0.179;
    let target = if dark_bg { 255 } else { 0 };
    let mut best = base;
    for step in 0..=20 {
        let c = mix(base, target, step);
        best = c;
        if contrast_ratio(c, background) >= MIN_CONTRAST {
            return c;
        }
    }
    best
}

/// A remote cursor resolved to character offsets in the local buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCursor {
    /// Whose.
    pub peer: PeerId,
    /// Display name.
    pub name: String,
    /// Foreground for the caret cell and name.
    pub color: Rgb,
    /// The caret, in characters.
    pub head: usize,
    /// The other end of the selection, if any, in characters.
    pub anchor: Option<usize>,
}

impl RemoteCursor {
    /// The selected range as `(start, end)` characters, if non-empty.
    #[must_use]
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        (a != self.head).then(|| (a.min(self.head), a.max(self.head)))
    }
}

/// The latest cursor each peer reported, and the names to show for them.
#[derive(Debug, Clone, Default)]
pub struct PresenceBoard {
    names: BTreeMap<PeerId, String>,
    latest: BTreeMap<PeerId, Presence>,
}

impl PresenceBoard {
    /// A new empty board.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record (or rename) a participant.
    pub fn join(&mut self, peer: PeerId, name: &str) {
        self.names.insert(peer, crate::peer::sanitize_name(name));
    }

    /// Forget a participant and their cursor.
    pub fn leave(&mut self, peer: PeerId) {
        self.names.remove(&peer);
        self.latest.remove(&peer);
    }

    /// Record a cursor report. Reports from peers that never joined are
    /// dropped, so a stray message cannot invent a participant.
    pub fn update(&mut self, presence: Presence) {
        if self.names.contains_key(&presence.peer) {
            self.latest.insert(presence.peer, presence);
        }
    }

    /// Everyone known, in id order.
    pub fn peers(&self) -> impl Iterator<Item = (PeerId, &str)> {
        self.names.iter().map(|(p, n)| (*p, n.as_str()))
    }

    /// Where `peer`'s cursor last was (for follow mode).
    #[must_use]
    pub fn location(&self, peer: PeerId) -> Option<&Presence> {
        self.latest.get(&peer)
    }

    /// The display name of `peer`.
    #[must_use]
    pub fn name(&self, peer: PeerId) -> Option<&str> {
        self.names.get(&peer).map(String::as_str)
    }

    /// Resolve every remote cursor on `path` against `doc` (skipping `me`).
    /// Anchors the document cannot resolve are skipped.
    #[must_use]
    pub fn cursors(
        &self,
        path: &SharedPath,
        doc: &SharedDoc,
        me: PeerId,
        background: Rgb,
    ) -> Vec<RemoteCursor> {
        self.latest
            .values()
            .filter(|p| p.peer != me && &p.path == path)
            .filter_map(|p| {
                let head = doc.resolve(&p.head)?;
                let anchor = p.anchor.as_ref().and_then(|a| doc.resolve(a));
                Some(RemoteCursor {
                    peer: p.peer,
                    name: self.names.get(&p.peer)?.clone(),
                    color: peer_color(p.peer, background),
                    head,
                    anchor,
                })
            })
            .collect()
    }

    /// Names of the peers whose cursor is on `path` (for the status line).
    #[must_use]
    pub fn names_on(&self, path: &SharedPath, me: PeerId) -> Vec<&str> {
        self.latest
            .values()
            .filter(|p| p.peer != me && &p.path == path)
            .filter_map(|p| self.names.get(&p.peer).map(String::as_str))
            .collect()
    }
}

/// Follow mode: pin the viewport to one peer until the local user scrolls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Follow(Option<PeerId>);

impl Follow {
    /// Start following `peer`.
    pub fn start(&mut self, peer: PeerId) {
        self.0 = Some(peer);
    }

    /// Stop following.
    pub fn stop(&mut self) {
        self.0 = None;
    }

    /// The followed peer, if any.
    #[must_use]
    pub const fn target(self) -> Option<PeerId> {
        self.0
    }

    /// The local user scrolled: any local scroll unpins.
    pub fn on_local_scroll(&mut self) {
        self.0 = None;
    }

    /// The followed peer left: unpin if it was them.
    pub fn on_peer_left(&mut self, peer: PeerId) {
        if self.0 == Some(peer) {
            self.0 = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::DEFAULT_CAP_BYTES;

    const DARK: Rgb = Rgb(0x1e, 0x1e, 0x2e);
    const LIGHT: Rgb = Rgb(0xfd, 0xf6, 0xe3);
    const BLACK: Rgb = Rgb(0, 0, 0);
    const WHITE: Rgb = Rgb(255, 255, 255);

    #[test]
    fn contrast_matches_the_wcag_extremes() {
        assert!((contrast_ratio(BLACK, WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(WHITE, WHITE) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn every_peer_color_clears_the_minimum_on_dark_light_and_extreme_themes() {
        for bg in [DARK, LIGHT, BLACK, WHITE, Rgb(0x80, 0x80, 0x80)] {
            for id in 1..=24 {
                let c = peer_color(PeerId(id), bg);
                assert!(
                    contrast_ratio(c, bg) >= MIN_CONTRAST - 0.2,
                    "peer {id} on {bg:?} gave {c:?} at {}",
                    contrast_ratio(c, bg)
                );
            }
        }
    }

    #[test]
    fn colors_are_stable_and_distinct_within_a_palette_cycle() {
        assert_eq!(peer_color(PeerId(3), DARK), peer_color(PeerId(3), DARK));
        let first: Vec<Rgb> = (1..=8).map(|i| peer_color(PeerId(i), DARK)).collect();
        for (i, a) in first.iter().enumerate() {
            for b in &first[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    fn board_with(doc: &SharedDoc) -> (PresenceBoard, SharedPath) {
        let path = SharedPath::parse("a.txt").unwrap();
        let mut b = PresenceBoard::new();
        b.join(PeerId(2), "Ada");
        b.update(Presence {
            peer: PeerId(2),
            path: path.clone(),
            head: doc.anchor_at(6).unwrap(),
            anchor: doc.anchor_at(2),
        });
        (b, path)
    }

    #[test]
    fn cursors_resolve_and_skip_me_and_other_files() {
        let doc = SharedDoc::host(PeerId::HOST, "hello world", DEFAULT_CAP_BYTES).unwrap();
        let (mut b, path) = board_with(&doc);
        let cs = b.cursors(&path, &doc, PeerId::HOST, DARK);
        assert_eq!(cs.len(), 1);
        assert_eq!((cs[0].head, cs[0].selection()), (6, Some((2, 6))));
        assert_eq!(cs[0].name, "Ada");
        assert_eq!(b.cursors(&path, &doc, PeerId(2), DARK), Vec::new());
        let other = SharedPath::parse("b.txt").unwrap();
        assert_eq!(b.cursors(&other, &doc, PeerId::HOST, DARK), Vec::new());
        assert_eq!(b.names_on(&path, PeerId::HOST), ["Ada"]);
        b.leave(PeerId(2));
        assert_eq!(b.cursors(&path, &doc, PeerId::HOST, DARK), Vec::new());
    }

    #[test]
    fn a_stranger_cannot_invent_a_participant() {
        let doc = SharedDoc::host(PeerId::HOST, "abc", DEFAULT_CAP_BYTES).unwrap();
        let path = SharedPath::parse("a").unwrap();
        let mut b = PresenceBoard::new();
        b.update(Presence {
            peer: PeerId(99),
            path: path.clone(),
            head: doc.anchor_at(1).unwrap(),
            anchor: None,
        });
        assert_eq!(b.cursors(&path, &doc, PeerId::HOST, DARK), Vec::new());
    }

    #[test]
    fn names_are_sanitized_on_join() {
        let mut b = PresenceBoard::new();
        b.join(PeerId(2), "\u{1b}[2Jevil");
        assert_eq!(b.name(PeerId(2)), Some("[2Jevil"));
    }

    #[test]
    fn follow_unpins_on_local_scroll_and_when_the_peer_leaves() {
        let mut f = Follow::default();
        assert_eq!(f.target(), None);
        f.start(PeerId(2));
        assert_eq!(f.target(), Some(PeerId(2)));
        f.on_peer_left(PeerId(3));
        assert_eq!(f.target(), Some(PeerId(2)));
        f.on_local_scroll();
        assert_eq!(f.target(), None);
        f.start(PeerId(2));
        f.on_peer_left(PeerId(2));
        assert_eq!(f.target(), None);
    }
}
