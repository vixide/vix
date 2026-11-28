//! Workspace search off the input thread (T869, audit PQ-08).
//!
//! A keystroke in the project-search panel used to scan the whole index
//! before the key handler returned (about 90 ms for a 10,000-file tree in a
//! release build, more on a cold cache or a slow disk). Now
//! [`App::request_workspace_search`] snapshots what the scan reads (the file
//! index, the workspace folders, the authority and every named buffer's
//! text, so the scan sees the buffers as they were at that keystroke) and
//! hands it to a worker thread. The worker posts hits as it finds them, in
//! index order, then a summary; [`App::poll_workspace_search`] (from
//! [`App::tick`]) installs them.
//!
//! Each request takes a new generation and replaces the job before it, whose
//! drop cancels its scan: an event from any other generation is discarded,
//! so a slow scan for an old query can never overwrite a newer one's hits.
//! Closing the panel drops the job too. Until the new generation's first
//! hits arrive the panel keeps showing the previous hits, so typing does not
//! flash an empty list. Owning spec: `spec/find-and-replace/index.md`
//! ("Responsiveness").

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use regex::Regex;

use super::App;
use super::remote_search::{self, IndexLine, Scan};
use crate::geo::Geo;
use crate::workspace_search::Hit;
use crate::workspace_search::tool::ToolQuery;

/// The most hits a search collects; the status line says the list was cut.
pub(super) const MAX_HITS: usize = 5_000;

/// How often a worker posts the hits it has found since its last post; the
/// first hit is posted at once.
const FLUSH: Duration = Duration::from_millis(50);

/// What a background search posts.
enum Event {
    /// Hits found since the last post, in index order.
    Hits(Vec<Hit>),
    /// The scan finished; this many files had hits.
    Done(usize),
}

/// A background workspace search in flight.
pub(crate) struct SearchJob {
    /// The request this job answers.
    generation: u64,
    /// Events tagged with the generation that posted them.
    rx: mpsc::Receiver<(u64, Event)>,
    /// Set on drop: the worker stops reading files.
    cancel: Arc<AtomicBool>,
    /// Whether this generation's hits replaced the panel's yet.
    seen: bool,
    /// The selection to restore when the search completes (a resumed
    /// picker), else the selection is only kept in range.
    select: Option<usize>,
    /// When the request was made, for [`SearchTiming`].
    started: Instant,
    /// When the first hits (or, with none, the summary) arrived.
    first: Option<Duration>,
}

impl Drop for SearchJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// How long the last completed background search took (T869): from the
/// request to its first posted hits (or to completion, without any) and to
/// completion. For benches and the latency example.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SearchTiming {
    /// Request to the first hits shown.
    pub first_result: Duration,
    /// Request to the summary.
    pub complete: Duration,
}

/// Visit every line `scan` reaches through the path `filter` and hand
/// each match of `re` to `sink` as a [`Hit`], in index order, stopping
/// at [`MAX_HITS`]. Returns how many files had a hit, counted as the
/// one-thread search always counted them ([`remote_search::FileCount`]).
pub(super) fn collect_hits(
    scan: &Scan<'_>,
    query: &ToolQuery,
    filter: &crate::find_panel::PathFilter,
    re: &Regex,
    sink: &mut dyn FnMut(Hit),
) -> usize {
    let mut files = remote_search::FileCount::default();
    let mut count = 0usize;
    scan.run(
        query,
        remote_search::Source::Buffers,
        &|rel| filter.allows(&rel.replace('\\', "/")),
        &mut |line: &IndexLine<'_>| {
            files.enter(line.path);
            let Some((start, _)) = crate::find_panel::next_match(line.text, re, 0) else {
                return ControlFlow::Continue(());
            };
            files.hit();
            let clipped = crate::workspace_search::clip(line.text);
            sink(Hit {
                path: line.path.to_path_buf(),
                rel: line.rel.to_string(),
                line: line.number,
                col: start.plus(1),
                display: format!("{}:{}: {clipped}", line.rel, line.number),
                text: clipped,
            });
            count = count.plus(1);
            if count >= MAX_HITS {
                files.cap();
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    );
    files.total()
}

/// The panel's status line for a finished search.
pub(super) fn summary(hits: usize, files: usize) -> String {
    if hits == 0 {
        t!("status.no_matches_cap").into()
    } else {
        t!("status.matches_in_files", count = hits, files = files).to_string()
    }
}

impl App {
    /// Search the panel's current pattern in the background (T869): the
    /// key handler returns at once and [`App::poll_workspace_search`]
    /// installs the hits. A search still running for an earlier pattern is
    /// cancelled and its hits discarded. Nothing to scan (see
    /// [`App::run_workspace_search`]) is handled here, on the spot.
    pub(super) fn request_workspace_search(&mut self) {
        self.request_workspace_search_selecting(None);
    }

    /// [`App::request_workspace_search`], restoring the selection `select`
    /// (kept in range) once the search completes.
    pub(super) fn request_workspace_search_selecting(&mut self, select: Option<usize>) {
        let Some((query, filter, re)) = self.workspace_search_plan() else {
            return;
        };
        let snapshot = self.scan_snapshot(remote_search::Source::Buffers);
        let cancel = Arc::clone(&snapshot.cancel);
        let generation = self.workspace_search_generation;
        self.workspace_search_generation = generation.wrapping_add(1);
        let (tx, rx) = mpsc::channel();
        let worker = move || {
            let scan = snapshot.scan();
            let mut batch: Vec<Hit> = Vec::new();
            let mut posted: Option<Instant> = None;
            let files = collect_hits(&scan, &query, &filter, &re, &mut |hit| {
                batch.push(hit);
                if posted.is_none_or(|t| t.elapsed() >= FLUSH) {
                    posted = Some(Instant::now());
                    if tx
                        .send((generation, Event::Hits(std::mem::take(&mut batch))))
                        .is_err()
                    {
                        // The job is gone: stop reading.
                        snapshot.cancel.store(true, Ordering::Relaxed);
                    }
                }
            });
            if scan.cancelled() {
                return;
            }
            if !batch.is_empty() {
                let _ = tx.send((generation, Event::Hits(batch)));
            }
            let _ = tx.send((generation, Event::Done(files)));
        };
        if std::thread::Builder::new()
            .name("vix-workspace-search".into())
            .spawn(worker)
            .is_err()
        {
            // No thread to spare: search here, as before T869.
            self.run_workspace_search();
            return;
        }
        if let Some(p) = self.workspace_search.as_mut() {
            p.status = t!("status.workspace_searching").to_string();
        }
        self.workspace_search_job = Some(SearchJob {
            generation,
            rx,
            cancel,
            seen: false,
            select,
            started: Instant::now(),
            first: None,
        });
    }

    /// Install what a background workspace search posted since the last
    /// call (T869). Called once per event-loop iteration; cheap when no
    /// search runs. A closed panel drops its search.
    pub fn poll_workspace_search(&mut self) {
        self.drain_workspace_search(false);
    }

    /// Block until the background workspace search (if any) completes and
    /// install its hits: for an action that needs the complete list
    /// (opening a hit, the results as a buffer), and for tests and benches.
    pub fn wait_for_workspace_search(&mut self) {
        self.drain_workspace_search(true);
    }

    /// The open project-search panel, if any: its query, hits and status
    /// line (for tests and benches, T869).
    #[must_use]
    pub fn workspace_search_panel(&self) -> Option<&crate::workspace_search::WorkspaceSearch> {
        self.workspace_search.as_ref()
    }

    /// Whether a background workspace search is running.
    #[must_use]
    pub fn workspace_search_busy(&self) -> bool {
        self.workspace_search_job.is_some()
    }

    /// How long the last background workspace search took, once it has
    /// completed; `None` before any has (T869).
    #[must_use]
    pub fn workspace_search_timing(&self) -> Option<SearchTiming> {
        self.workspace_search_timing
    }

    /// Apply the job's events; with `block`, wait for them until it ends.
    fn drain_workspace_search(&mut self, block: bool) {
        // A closed panel, or one replaced by a fixed list (definitions,
        // references), wants no hits from a search of the old pattern.
        if self
            .workspace_search
            .as_ref()
            .is_none_or(|p| p.flags.contains(super::WorkspaceFlags::STATIC_RESULTS))
        {
            self.workspace_search_job = None;
            return;
        }
        loop {
            let Some(job) = self.workspace_search_job.as_mut() else {
                return;
            };
            let next = if block {
                job.rx.recv().map_err(|_| TryRecvError::Disconnected)
            } else {
                job.rx.try_recv()
            };
            let (generation, event) = match next {
                Ok(e) => e,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // The worker died without a summary: search here.
                    self.workspace_search_job = None;
                    self.run_workspace_search();
                    return;
                }
            };
            if generation != job.generation {
                continue; // a stale generation's event: discard it
            }
            let elapsed = job.started.elapsed();
            let first = *job.first.get_or_insert(elapsed);
            let seen = std::mem::replace(&mut job.seen, true);
            let select = job.select;
            let Some(p) = self.workspace_search.as_mut() else {
                self.workspace_search_job = None;
                return;
            };
            match event {
                Event::Hits(hits) => {
                    if seen {
                        p.hits.extend(hits);
                    } else {
                        p.hits = hits;
                    }
                    if p.selected >= p.hits.len() {
                        p.selected = p.hits.len().minus(1);
                    }
                }
                Event::Done(files) => {
                    if !seen {
                        p.hits.clear();
                    }
                    p.status = summary(p.hits.len(), files);
                    p.selected = select.unwrap_or(p.selected).min(p.hits.len().minus(1));
                    self.workspace_search_timing = Some(SearchTiming {
                        first_result: first,
                        complete: elapsed,
                    });
                    self.workspace_search_job = None;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, SearchJob};
    use crate::app::App;
    use crate::settings::Settings;
    use crate::workspace_search::{Hit, WorkspaceSearch};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Instant;

    fn hit(rel: &str) -> Hit {
        Hit {
            path: PathBuf::from(rel),
            rel: rel.to_string(),
            line: 1,
            col: 1,
            display: format!("{rel}:1: x"),
            text: "x".to_string(),
        }
    }

    /// An App with an open panel showing `old` and a job for generation 7.
    fn app_with_job(old: &[&str]) -> (App, mpsc::Sender<(u64, Event)>, Arc<AtomicBool>) {
        let mut app = App::new(PathBuf::from("/nonexistent-t869"), Settings::default());
        let mut panel = WorkspaceSearch::new(false);
        panel.query = "xx".to_string();
        panel.hits = old.iter().map(|r| hit(r)).collect();
        app.workspace_search = Some(panel);
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        app.workspace_search_job = Some(SearchJob {
            generation: 7,
            rx,
            cancel: Arc::clone(&cancel),
            seen: false,
            select: None,
            started: Instant::now(),
            first: None,
        });
        (app, tx, cancel)
    }

    fn rels(app: &App) -> Vec<String> {
        app.workspace_search_hits()
            .iter()
            .map(|h| h.rel.clone())
            .collect()
    }

    #[test]
    fn another_generations_events_are_discarded() {
        let (mut app, tx, _) = app_with_job(&["old"]);
        tx.send((6, Event::Hits(vec![hit("stale")]))).unwrap();
        tx.send((6, Event::Done(1))).unwrap();
        app.poll_workspace_search();
        assert_eq!(rels(&app), ["old"], "the old hits stay until new ones come");
        assert!(app.workspace_search_busy());
        tx.send((7, Event::Hits(vec![hit("a")]))).unwrap();
        app.poll_workspace_search();
        assert_eq!(rels(&app), ["a"], "the first new hits replace the old");
        tx.send((7, Event::Hits(vec![hit("b")]))).unwrap();
        tx.send((7, Event::Done(2))).unwrap();
        app.poll_workspace_search();
        assert_eq!(rels(&app), ["a", "b"], "later hits append, in order");
        assert!(!app.workspace_search_busy());
        assert!(app.workspace_search_timing().is_some());
    }

    #[test]
    fn a_search_with_no_hits_clears_the_old_ones() {
        let (mut app, tx, _) = app_with_job(&["old"]);
        tx.send((7, Event::Done(0))).unwrap();
        app.wait_for_workspace_search();
        assert_eq!(rels(&app), Vec::<String>::new());
        assert!(!app.workspace_search_busy());
    }

    #[test]
    fn a_fixed_list_replacing_the_panel_gets_no_hits() {
        let (mut app, tx, cancel) = app_with_job(&["def"]);
        if let Some(p) = app.workspace_search.as_mut() {
            p.flags.insert(super::super::WorkspaceFlags::STATIC_RESULTS);
        }
        tx.send((7, Event::Hits(vec![hit("search")]))).unwrap();
        app.poll_workspace_search();
        assert_eq!(rels(&app), ["def"]);
        assert!(!app.workspace_search_busy());
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn closing_the_panel_cancels_the_scan() {
        let (mut app, _tx, cancel) = app_with_job(&[]);
        app.on_key(vix_input::KeyEvent::new(
            vix_input::KeyCode::Esc,
            vix_input::KeyModifiers::NONE,
        ));
        assert!(app.workspace_search.is_none());
        assert!(!app.workspace_search_busy());
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn a_worker_that_dies_is_replaced_by_a_search_here() {
        let (mut app, tx, _) = app_with_job(&["old"]);
        drop(tx);
        app.poll_workspace_search();
        assert!(!app.workspace_search_busy());
        assert_eq!(rels(&app), Vec::<String>::new(), "nothing indexed: no hits");
    }
}
