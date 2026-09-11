# Coverage

Parse LCOV and Cobertura XML coverage reports into per-file, per-line hit
data for the **coverage gutter** (Tools → Load Coverage File…). Vix
visualizes an existing report; it doesn't run a coverage tool itself — the
user generates one first (`cargo llvm-cov`, `pytest --cov`, `go test
-coverprofile`, …).

## Supported formats

- **LCOV** (`.info`, the `lcov`/`genhtml` text format): `SF:` names the file
  a block of records covers, `DA:<line>,<count>` gives each line's hit
  count, `BRDA:<line>,<block>,<branch>,<taken>` gives per-branch data, and
  `end_of_record` closes the block. A line with any `BRDA:` branch whose
  `taken` is `-` or `0` is reported partial; otherwise a nonzero `DA:` count
  is covered and a zero count is uncovered.
- **Cobertura XML**: `<class filename="…">` names the file for the
  `<line number="…" hits="…">` elements nested under it.
  `branch="true" condition-coverage="…% (a/b)"` with `a < b` is reported
  partial; a `hits="0"` line is uncovered; anything else hit is covered.

`parse` auto-detects which format `text` is (an LCOV file never starts with
`<`). The Cobertura reader is a tolerant single-pass token scan, not a
validating XML parser — it reads the `filename`/`number`/`hits`/
`branch`/`condition-coverage` attributes it needs in document order and
ignores everything else about the document's structure or nesting depth.

## Matching a report's file names to an open buffer

Reports record file paths inconsistently: relative to the project root,
relative to wherever the tool ran, or with `\` separators on Windows.
`Report::lines_for` tries an exact match first, then falls back to one path
being a suffix of the other (normalized to `/`) — so a report generated in
CI with a different checkout path still matches a locally opened buffer,
as long as the trailing path segments agree.

## As implemented in Vix

- `crate::coverage` (this crate) is pure parsing: `parse`/`parse_lcov`/
  `parse_cobertura` into a `Report`, and `Report::lines_for`/`file_count`.
  No I/O, no `App` dependency; unit tested against both formats plus the
  suffix-matching fallback.
- The host (`App`, `src/app/coverage.rs`) reads the file (**Tools → Load
  Coverage File…**, prompting for a path, pre-filled from the
  `coverage_path` setting when set), keeps the parsed `Report` and a
  visibility flag, and reuses the diff-gutter's existing `(line, Color)`
  mark mechanism (`Editor::set_gutter_marks`) to paint covered/uncovered/
  partial bars in the gutter — green/red/yellow, the same hex values the
  git diff gutter uses for added/deleted/modified. The coverage gutter and
  the git diff gutter share that one gutter-sign column, so only one shows
  at a time: while coverage is visible, the git diff refresh is skipped for
  the active tab. **Tools → Toggle Coverage Gutter** flips visibility
  without re-parsing (the loaded `Report` stays cached).
