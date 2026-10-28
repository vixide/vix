# Git Graph

The interactive Git graph: commit lanes from parent links, ref decorations, paged log loading and per-commit actions.

**Status:** Shipped (improvement plan T681). **Git → Log → Graph…**
(`git.graph`) opens an overlay that draws the repository's history as a
graph with branch lanes. It sits beside the plain streamed **Git → Log →
Graph** text view (`git.log_graph`), which only pipes `git log --graph` into
the bottom dock and stays as it was.

## Layers

The crate is three layers, each usable and testable alone:

1. **Parsing** -- `parse_log` reads `git log` output in the crate's own format
   (`LOG_FORMAT`: hash, abbreviated hash, parents, author, short date, ref
   names, subject; fields split by `\x1f`, records by `\x1e`) into `Commit`s.
   `parse_decorations` reads the `%D` field produced with `--decorate=full`
   into `RefLabel`s (`RefKind` is `Head`, `Branch`, `Remote` or `Tag`;
   `origin/HEAD` is dropped).
2. **Lanes** -- `assign_lanes(&[Commit]) -> Vec<Row>` is a pure function. The
   input is newest first with every parent after its children (what
   `git log --topo-order` gives). It keeps one slot per open lane, each
   waiting for a specific hash:
   - a commit takes the leftmost lane waiting for it, or the first free lane
     when it is a branch tip;
   - every lane waiting for it ends there (`Row::incoming`);
   - its first parent continues in the commit's own lane, other parents take
     the lane already waiting for them or the first free one
     (`Row::outgoing`);
   - lanes that merely pass by are `Row::through`.
   A parent missing from the slice (history cut off by paging) keeps its lane
   open to the bottom. Appending commits never changes the rows already
   computed, so loading a page cannot redraw earlier rows differently.
   `Row::glyphs(width)` draws a row: `●` the commit, `│` a passing lane, `╯`
   a lane merging in, `╮`/`╭` a lane branching off, `─` joining them.
3. **Panel** -- `Panel` holds the loaded commits and rows, the search `query`,
   the branch/tag `reference` filter, the highlight, and a `Mode`.

With the `process` feature (default; the root `git` feature turns it on) the
crate also runs git: `fetch_page(dir, skip, count, reference)`
(`git log --topo-order --decorate=full --skip N -n 200`, across `--all` or one
ref), `list_refs`, `checkout`, `create_branch_at`, `cherry_pick`. Every
failure is a `GraphError` (`NotInstalled`, `Io`, `Failed { code, stderr }`,
`InvalidArgument`, `Parse`), the same shape as `vix_git::GitError`. Names and
hashes are validated before git runs and passed after `--end-of-options`.

## The overlay

- **Paging.** The first page is 200 commits (`PAGE_SIZE`); the next page loads
  when the highlight comes within 20 rows of the end of what is loaded, until
  git returns a short page. A header shows `…` while more remain.
- **Search.** Typing filters the loaded commits by hash, author, subject or
  ref name (case-insensitive); `Backspace` edits. Lanes are computed over
  every loaded commit, so hiding rows never breaks the lines between those
  that remain.
- **Branch/tag filter.** `Tab` / `Shift+Tab` step through *all refs*, then each
  local branch, remote branch and tag; each step reloads the first page.
- **Open diff.** `Enter` (or clicking the highlighted row) opens the commit as
  a read-only `git show` tab titled `<abbrev> graph` -- the T207 machinery
  (`vix_git::show_commit`, `open_readonly_text_tab`) -- and closes the panel.
- **Checkout.** `Ctrl+O` asks `y/n`, then switches to the commit's local
  branch when it has one that `HEAD` is not already on, else detaches `HEAD`
  at the commit.
- **New branch.** `Ctrl+B` asks for a name, then `git switch -c <name> <hash>`.
- **Cherry-pick.** `Ctrl+P` asks `y/n`, then cherry-picks onto the current
  branch. When git stops (a conflict, an empty pick) the pick is aborted so
  the repository is left as it was, and the error is shown.
- **Copy hash.** `Ctrl+Y` copies the full hash.
- `Esc` cancels a pending question or name, and otherwise closes the panel.

After a checkout, branch or cherry-pick the git state, explorer and clean
buffers refresh and the graph reloads.

## Tests

The lanes test is the property test: over random commit DAGs every lane an
earlier commit opened toward a hash arrives at that hash's row (`incoming`),
the node takes the leftmost of them, every lane fits the row's width, there is
one outgoing lane per parent, glyph rows are two characters per column, and a
prefix of the history gets identical rows. The repository test builds throwaway
repositories with git commands: paging, decorations, the ref filter, an empty
repository, checkout (branch and detached), new branch, cherry-pick and its
abort. The `vix-tui` integration tests drive the overlay by key.

## Not done

- Lane colors (the graph is drawn in one dim color).
- Horizontal scrolling for very wide graphs; the graph column is not capped.
- Remote operations (push/fetch) from a commit.
