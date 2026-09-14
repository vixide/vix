//! Chapter metadata and bundled lesson bodies (`spec/index.md`, § Chapters
//! and lessons, § Working copy).

/// One tutorial chapter: metadata only. The prose lives in
/// `crates/vix-tutor/lessons/`, pulled in via [`Chapter::body`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chapter {
    /// Stable slug, also the working-copy filename's stem (`"moving-around"`).
    pub id: &'static str,
    /// The i18n key for this chapter's human title.
    pub title: &'static str,
    /// 0-based position in [`CHAPTERS`].
    pub ordinal: usize,
}

impl Chapter {
    /// This chapter's bundled starter text, embedded at compile time from
    /// `crates/vix-tutor/lessons/`. The host copies this into a fresh working
    /// file on open, and back over it again on `tutor.restart_chapter`.
    #[must_use]
    pub fn body(&self) -> &'static str {
        match self.id {
            "moving-around" => include_str!("../lessons/01-moving-around.txt"),
            "editing-basics" => include_str!("../lessons/02-editing-basics.txt"),
            "find-and-replace" => include_str!("../lessons/03-find-and-replace.txt"),
            "multi-cursor-and-selection" => {
                include_str!("../lessons/04-multi-cursor-and-selection.txt")
            }
            "files-tabs-and-palette" => include_str!("../lessons/05-files-tabs-and-palette.txt"),
            "git-basics" => include_str!("../lessons/06-git-basics.txt"),
            _ => "",
        }
    }

    /// The working-copy filename for this chapter, e.g. `"01-moving-around.txt"`
    /// — matches the bundled lesson file's own name.
    #[must_use]
    pub fn filename(&self) -> String {
        format!("{:02}-{}.txt", self.ordinal + 1, self.id)
    }
}

/// Every tutorial chapter, in order (`spec/index.md`, § Chapters and
/// lessons).
pub const CHAPTERS: &[Chapter] = &[
    Chapter {
        id: "moving-around",
        title: "tutor.chapter.moving_around",
        ordinal: 0,
    },
    Chapter {
        id: "editing-basics",
        title: "tutor.chapter.editing_basics",
        ordinal: 1,
    },
    Chapter {
        id: "find-and-replace",
        title: "tutor.chapter.find_and_replace",
        ordinal: 2,
    },
    Chapter {
        id: "multi-cursor-and-selection",
        title: "tutor.chapter.multi_cursor_and_selection",
        ordinal: 3,
    },
    Chapter {
        id: "files-tabs-and-palette",
        title: "tutor.chapter.files_tabs_and_palette",
        ordinal: 4,
    },
    Chapter {
        id: "git-basics",
        title: "tutor.chapter.git_basics",
        ordinal: 5,
    },
];

/// The chapter with this `id`, if any.
#[must_use]
pub fn chapter(id: &str) -> Option<&'static Chapter> {
    CHAPTERS.iter().find(|c| c.id == id)
}

/// The chapter at this 0-based `ordinal`, if any.
#[must_use]
pub fn by_ordinal(ordinal: usize) -> Option<&'static Chapter> {
    CHAPTERS.get(ordinal)
}

#[cfg(test)]
mod tests {
    use super::{CHAPTERS, by_ordinal, chapter};

    #[test]
    fn chapters_are_ordered_from_zero() {
        for (i, c) in CHAPTERS.iter().enumerate() {
            assert_eq!(c.ordinal, i);
        }
    }

    #[test]
    fn chapter_looks_up_by_id() {
        assert_eq!(chapter("moving-around").unwrap().ordinal, 0);
        assert!(chapter("no-such-chapter").is_none());
    }

    #[test]
    fn by_ordinal_looks_up_and_bounds_checks() {
        assert_eq!(by_ordinal(0).unwrap().id, "moving-around");
        assert!(by_ordinal(CHAPTERS.len()).is_none());
    }

    #[test]
    fn every_chapter_has_a_non_empty_body_and_filename() {
        for c in CHAPTERS {
            assert!(!c.body().is_empty(), "{} has no bundled body", c.id);
            assert!(
                std::path::Path::new(&c.filename())
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
            );
        }
    }
}
