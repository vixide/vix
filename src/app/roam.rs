//! Org-roam and Org-node: node lookup/creation, wiki-link insertion,
//! random-node jump, live backlinks, dailies (open/capture a daily note),
//! the node graph, database sync, and the `node.*` transforms (nodeify,
//! extract subtree, insert transclusion, rename by title, dead-links,
//! reset).
//!
//! Moved out of `app.rs` verbatim (T141, slice 7 -- the first of several
//! planned sub-slices of the much larger "org" area, which turned out
//! too big to move in one piece; see `tasks.md`'s T141 entry). Mostly
//! contiguous, plus one outlier (`refresh_backlinks_follow`).

#![warn(clippy::pedantic)]

use std::path::{Path, PathBuf};

use super::{App, Prompt, PromptKind};

impl App {
    /// Dispatch an Org-roam / Org-node action. Returns `true` if `action` was
    /// handled. Extracted to keep [`App::org_action`] within the line limit.
    pub(super) fn roam_action(&mut self, action: &str) -> bool {
        match action {
            "roam.node_find" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamFind,
                    t!("prompt.roam_find").to_string(),
                ));
            }
            "roam.node_insert" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamInsert,
                    t!("prompt.roam_insert").to_string(),
                ));
            }
            "roam.node_random" => self.roam_node_random(),
            "roam.capture" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamCapture,
                    t!("prompt.roam_capture").to_string(),
                ));
            }
            "roam.backlinks" => self.roam_backlinks(),
            "roam.backlinks_follow" => {
                self.backlinks_follow = !self.backlinks_follow;
                self.backlinks_follow_key = None; // force a rebuild on next refresh
                self.refresh_backlinks_follow();
                self.status = t!("status.backlinks_follow", on = self.backlinks_follow).to_string();
            }
            "roam.dailies_today" => self.roam_open_daily(&Self::roam_today()),
            "roam.dailies_calendar" => {
                self.calendar = crate::calendar::Calendar::new();
                self.calendar_dailies = true;
                self.show_calendar = true;
            }
            "roam.dailies_capture" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamDailyCapture,
                    t!("prompt.roam_daily_capture").to_string(),
                ));
            }
            "roam.dailies_date" => {
                self.prompt = Some(
                    Prompt::new(
                        PromptKind::RoamDailyDate,
                        t!("prompt.roam_daily_date").to_string(),
                    )
                    .with_input(Self::roam_today()),
                );
            }
            "roam.tag_add" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamTag,
                    t!("prompt.roam_tag").to_string(),
                ));
            }
            "roam.alias_add" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamAlias,
                    t!("prompt.roam_alias").to_string(),
                ));
            }
            "roam.ref_add" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RoamRef,
                    t!("prompt.roam_ref").to_string(),
                ));
            }
            "roam.graph" => self.roam_graph(),
            "roam.db_sync" => self.roam_db_sync(),
            "node.nodeify" => self.node_nodeify(),
            "node.extract_subtree" => self.node_extract_subtree(),
            "node.insert_transclusion" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::NodeTransclusion,
                    t!("prompt.node_transclusion").to_string(),
                ));
            }
            "node.rename_by_title" => self.node_rename_by_title(),
            "node.dead_links" => self.node_dead_links(),
            "node.reset" => self.node_reset(),
            "roam.link_complete" => self.open_node_link_completion(),
            _ => return false,
        }
        true
    }

    /// Today's date as `YYYY-MM-DD` in the local zone.
    fn roam_today() -> String {
        jiff::Zoned::now().strftime("%Y-%m-%d").to_string()
    }

    /// Every `.org` file in the project as `(relative-name, content)` pairs.
    pub(super) fn roam_node_files(&self) -> Vec<(String, String)> {
        let mut files = Vec::new();
        for path in &self.file_index {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("org"))
                && let Ok(content) = std::fs::read_to_string(path)
            {
                let name = path
                    .strip_prefix(&self.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                files.push((name, content));
            }
        }
        files
    }

    /// Find a node by title (case-insensitive) among the project's `.org` files.
    fn roam_find_by_title(&self, title: &str) -> Option<PathBuf> {
        let want = title.trim().to_ascii_lowercase();
        for path in &self.file_index {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("org"))
                && let Ok(content) = std::fs::read_to_string(path)
                && crate::roam::node_title(&content).is_some_and(|t| t.to_ascii_lowercase() == want)
            {
                return Some(path.clone());
            }
        }
        None
    }

    /// Write `content` to `path`, then open it and refresh the file index. Reports
    /// failures on the message line. Returns whether the write succeeded.
    pub(super) fn roam_write_and_open(&mut self, path: &Path, content: &str) -> bool {
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.messages
                .error(t!("msg.save_failed", error = e).to_string());
            return false;
        }
        if let Err(e) = std::fs::write(path, content) {
            self.messages
                .error(t!("msg.save_failed", error = e).to_string());
            return false;
        }
        self.build_file_index();
        self.explorer.rebuild();
        self.open_path(path, false);
        true
    }

    /// Find a node by title and open it, or create a new node file for it.
    /// Returns the node's `:ID:` (existing or freshly minted) when available.
    pub(super) fn roam_visit_or_create(&mut self, title: &str) -> Option<String> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        if let Some(path) = self.roam_find_by_title(title) {
            let id = std::fs::read_to_string(&path)
                .ok()
                .and_then(|c| crate::roam::node_id(&c));
            self.open_path(&path, false);
            return id;
        }
        let id = crate::uuid_tool::v4();
        let path = self
            .root
            .join(format!("{}.org", crate::roam::slugify(title)));
        let body = crate::roam::new_node(title, &id);
        if self.roam_write_and_open(&path, &body) {
            self.status = t!("status.roam_created", title = title).to_string();
            Some(id)
        } else {
            None
        }
    }

    /// Org-roam: insert a link to a node (found or created) at the cursor. Unlike
    /// find/capture this never leaves the buffer the user is editing — a freshly
    /// created node file is written to disk but not opened.
    pub(super) fn roam_insert_link(&mut self, title: &str) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let id = if let Some(path) = self.roam_find_by_title(&title) {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|c| crate::roam::node_id(&c))
        } else {
            let id = crate::uuid_tool::v4();
            let path = self
                .root
                .join(format!("{}.org", crate::roam::slugify(&title)));
            let body = crate::roam::new_node(&title, &id);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&path, &body) {
                Ok(()) => {
                    self.build_file_index();
                    self.explorer.rebuild();
                    Some(id)
                }
                Err(e) => {
                    self.messages
                        .error(t!("msg.save_failed", error = e).to_string());
                    None
                }
            }
        };
        let link = id.map_or_else(
            || format!("[[file:{}.org][{title}]]", crate::roam::slugify(&title)),
            |id| crate::roam::node_link(&id, &title),
        );
        self.insert_content(&link);
    }

    /// Org-roam: jump to a randomly chosen node.
    fn roam_node_random(&mut self) {
        let paths: Vec<PathBuf> = self
            .file_index
            .iter()
            .filter(|p| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("org"))
                    && std::fs::read_to_string(p)
                        .ok()
                        .and_then(|c| crate::roam::node_title(&c))
                        .is_some()
            })
            .cloned()
            .collect();
        if paths.is_empty() {
            self.status = t!("status.roam_no_nodes").to_string();
            return;
        }
        let seed = usize::try_from(
            jiff::Zoned::now()
                .timestamp()
                .subsec_nanosecond()
                .unsigned_abs(),
        )
        .unwrap_or(0);
        let path = paths[seed % paths.len()].clone();
        self.open_path(&path, false);
    }

    /// Org-roam: compile a backlinks buffer for the active node into a new tab.
    fn roam_backlinks(&mut self) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let id = crate::roam::node_id(&text).unwrap_or_default();
        let title = crate::roam::node_title(&text).unwrap_or_default();
        if id.is_empty() && title.is_empty() {
            self.status = t!("status.roam_not_node").to_string();
            return;
        }
        let files = self.roam_node_files();
        let buffer = crate::roam::backlinks(&id, &title, &files);
        self.editor.new_tab_with_content(&buffer);
        self.status = t!("status.roam_backlinks", title = title).to_string();
    }

    /// Org-roam: open (creating if needed) the daily note for `date`.
    pub(super) fn roam_open_daily(&mut self, date: &str) {
        // Reject anything that isn't a real `YYYY-MM-DD`, so a prompt like
        // `../../etc/x` can't be turned into a path outside the notes directory.
        let Some(filename) = crate::roam::daily_filename(date) else {
            self.status = t!("status.roam_invalid_date", date = date).to_string();
            return;
        };
        let path = self.root.join(crate::roam::DAILIES_DIR).join(filename);
        if path.exists() {
            self.open_path(&path, false);
            return;
        }
        let body = crate::roam::daily_template(date, &crate::uuid_tool::v4());
        if self.roam_write_and_open(&path, &body) {
            self.status = t!("status.roam_created", title = date).to_string();
        }
    }

    /// Org-roam: append a timestamped entry to today's daily note, opening it.
    pub(super) fn roam_daily_capture(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let date = Self::roam_today();
        // `roam_today` always yields a valid `YYYY-MM-DD`; bail defensively if not.
        let Some(filename) = crate::roam::daily_filename(&date) else {
            return;
        };
        let path = self.root.join(crate::roam::DAILIES_DIR).join(filename);
        let time = jiff::Zoned::now().strftime("%H:%M").to_string();
        let entry = crate::roam::daily_entry(&time, text);
        let mut content = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| crate::roam::daily_template(&date, &crate::uuid_tool::v4()));
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&entry);
        if self.roam_write_and_open(&path, &content) {
            self.status = t!("status.roam_daily_captured").to_string();
        }
    }

    /// Rewrite the active buffer's content through `f` (an org-roam metadata edit).
    pub(super) fn roam_rewrite_active(&mut self, f: impl FnOnce(&str) -> String) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let new = f(&text);
        if new != text {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        }
    }

    /// Org-roam: build a Mermaid graph of all nodes and links into a new tab.
    fn roam_graph(&mut self) {
        let files = self.roam_node_files();
        let graph = crate::roam::graph(&files);
        self.editor.new_tab_with_content(&graph);
        self.status = t!("status.roam_graph").to_string();
    }

    /// Org-roam: refresh the file index and open a node-index buffer.
    fn roam_db_sync(&mut self) {
        self.build_file_index();
        let files = self.roam_node_files();
        let index = crate::roam::index(&files);
        let count = files
            .iter()
            .filter(|(_, c)| crate::roam::node_title(c).is_some())
            .count();
        self.editor.new_tab_with_content(&index);
        self.status = t!("status.roam_synced", count = count).to_string();
    }

    /// Org-node: give the headline at the cursor an `:ID:`, making it a node.
    fn node_nodeify(&mut self) {
        let id = crate::uuid_tool::v4();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = crate::roam::nodeify(&text, line, &id) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
            self.status = t!("status.node_nodeified").to_string();
        } else {
            self.status = t!("status.node_not_headline").to_string();
        }
    }

    /// Org-node: cut the subtree at the cursor into its own node file, leaving an
    /// `[[id:…]]` link in its place.
    fn node_extract_subtree(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let text = tab.text();
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let lines: Vec<&str> = text.split('\n').collect();
        let Some((start, end)) = crate::org::subtree_range(&lines, line) else {
            self.status = t!("status.org_not_headline").to_string();
            return;
        };
        let depth = crate::org::headline_level(lines[start]).unwrap_or(1);
        // The heading text becomes the new node's title (drop stars + TODO keyword).
        let mut heading = lines[start][depth..].trim().to_string();
        for kw in ["TODO ", "DONE "] {
            if let Some(rest) = heading.strip_prefix(kw) {
                heading = rest.to_string();
            }
        }
        // Body = the subtree's lines after the heading, each promoted by `depth-1`
        // leading stars so nested headlines stay relative to a top-level file.
        let body: String = lines[start + 1..end]
            .iter()
            .map(|l| {
                crate::org::headline_level(l).map_or_else(
                    || (*l).to_string(),
                    |lvl| {
                        format!(
                            "{} {}",
                            "*".repeat(lvl.saturating_sub(depth - 1).max(1)),
                            l[lvl..].trim_start()
                        )
                    },
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let id = crate::uuid_tool::v4();
        let path = self
            .root
            .join(format!("{}.org", crate::roam::slugify(&heading)));
        let mut file_body = crate::roam::new_node(&heading, &id);
        if !body.trim().is_empty() {
            file_body.push_str(&body);
            if !file_body.ends_with('\n') {
                file_body.push('\n');
            }
        }
        // Replace the subtree in the current buffer with a link to the new node.
        let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
        kept.extend_from_slice(&lines[..start]);
        let placeholder = format!(
            "{} {}",
            "*".repeat(depth),
            crate::roam::node_link(&id, &heading)
        );
        kept.push(&placeholder);
        kept.extend_from_slice(&lines[end..]);
        let remaining = kept.join("\n");
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&remaining);
            tab.editor.set_cursor_line(start);
            tab.dirty = true;
        }
        if self.roam_write_and_open(&path, &file_body) {
            self.status = t!("status.node_extracted", title = heading).to_string();
        }
    }

    /// Org-node: insert a `#+transclude:` directive for a node (found or created),
    /// without leaving the current buffer.
    pub(super) fn node_insert_transclusion(&mut self, title: &str) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let id = if let Some(path) = self.roam_find_by_title(&title) {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|c| crate::roam::node_id(&c))
        } else {
            let id = crate::uuid_tool::v4();
            let path = self
                .root
                .join(format!("{}.org", crate::roam::slugify(&title)));
            let body = crate::roam::new_node(&title, &id);
            match std::fs::write(&path, &body) {
                Ok(()) => {
                    self.build_file_index();
                    self.explorer.rebuild();
                    Some(id)
                }
                Err(e) => {
                    self.messages
                        .error(t!("msg.save_failed", error = e).to_string());
                    None
                }
            }
        };
        if let Some(id) = id {
            self.insert_content(&format!("{}\n", crate::roam::transclusion(&id, &title)));
        }
    }

    /// Org-node: rename the active file to the slug of its `#+title:`.
    fn node_rename_by_title(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let Some(title) = crate::roam::node_title(&text) else {
            self.status = t!("status.node_no_title").to_string();
            return;
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("org");
        let target = path.with_file_name(format!("{}.{ext}", crate::roam::slugify(&title)));
        if target == path {
            return;
        }
        match std::fs::rename(&path, &target) {
            Ok(()) => {
                self.editor.close_active();
                self.build_file_index();
                self.explorer.rebuild();
                self.open_path(&target, false);
                self.status = t!("status.node_renamed", path = target.display()).to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.save_failed", error = e).to_string()),
        }
    }

    /// Org-node: report `id:` links whose target node no longer exists.
    fn node_dead_links(&mut self) {
        let files = self.roam_node_files();
        let report = crate::roam::dead_links(&files);
        self.editor.new_tab_with_content(&report);
        self.status = t!("status.node_dead_links").to_string();
    }

    /// Org-node: rebuild the in-memory node index (org-mem-reset equivalent).
    fn node_reset(&mut self) {
        self.build_file_index();
        let count = self
            .roam_node_files()
            .iter()
            .filter(|(_, c)| crate::roam::node_id(c).is_some())
            .count();
        self.status = t!("status.node_reset", count = count).to_string();
    }

    /// When "Live Backlinks" is on, rebuild the bottom dock with the active node's
    /// backlinks whenever the active buffer (or its content) changes.
    pub fn refresh_backlinks_follow(&mut self) {
        if !self.backlinks_follow {
            return;
        }
        let key = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| (self.editor.active, t.editor.revision()));
        if key == self.backlinks_follow_key {
            return;
        }
        self.backlinks_follow_key = key;
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let id = crate::roam::node_id(&text).unwrap_or_default();
        let title = crate::roam::node_title(&text).unwrap_or_default();
        self.bottom_dock.clear();
        if id.is_empty() && title.is_empty() {
            self.bottom_dock
                .push(t!("status.roam_not_node").to_string());
        } else {
            let files = self.roam_node_files();
            for line in crate::roam::backlinks(&id, &title, &files).lines() {
                self.bottom_dock.push(line.to_string());
            }
        }
        self.show_bottom_dock = true;
    }
}
