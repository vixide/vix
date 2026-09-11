# Coverage Gutter

See at a glance which lines a test run actually covered, right in the
editor gutter — without leaving Vix™. Vix visualizes an existing coverage
report; it doesn't run a coverage tool itself.

## Generating a report

Run whatever coverage tool your project uses, and have it write an **LCOV**
(`.info`) or **Cobertura XML** report:

```sh
cargo llvm-cov --lcov --output-path coverage.info
pytest --cov --cov-report=xml           # coverage.xml, Cobertura format
go test -coverprofile=coverage.out && gocov convert coverage.out | gocov-xml > coverage.xml
```

## Using it

**Tools → Load Coverage File…** prompts for the report's path (relative
paths resolve from the workspace root) and shows the gutter immediately.
Each line gets a colored bar:

- **Green** — covered (executed).
- **Red** — uncovered (never executed).
- **Yellow** — partially covered (executed, but not every branch was
  taken — only reported when the format records branch data).

**Tools → Toggle Coverage Gutter** shows/hides it again without re-reading
the file — reload with Load Coverage File… after a fresh test run.

The coverage gutter and the [git diff gutter](../git-panel/index.md) share
one gutter-sign column, so only one shows per buffer at a time: while
coverage is visible, the diff gutter is skipped for that tab.

Set the **`coverage_path`** setting to pre-fill Load Coverage File…'s
prompt with your project's usual report path, so accepting it is a single
keystroke after a test run.

See the specification at `crates/vix-coverage/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
