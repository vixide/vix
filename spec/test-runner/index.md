# Test Runner

**Tools → Run Tests** (`tools.test`) runs the configured `test_command` through
the async Run Command pipeline, captures its output, and parses it into a
pass/fail list shown in the **Test panel** (toggle with `tools.test_panel`).

## Configuration

```toml
test_command = "cargo test"   # or "pytest -v", "npm test", …
test_width = 40                # test panel width
```

## Behavior

- Output streams to the bottom dock as usual; in parallel it is buffered and, on
  completion, parsed by `test_runner::parse`.
- Recognized formats: libtest (`test <name> ... ok|FAILED|ignored`) with failure
  locations from `---- <name> stdout ----` + `panicked at <file>:<line>` blocks,
  and the generic `<name> PASSED|FAILED|SKIPPED` shape (pytest `-v`, etc.).
- The panel lists results sorted by name (grouping by module prefix), each with a
  ✓ / ✗ / ○ icon. **Click** a row to jump to its failure location (when known).
- On completion a notification reports the tally (Info, or Error if any failed)
  via `status.tests_done`.

## As implemented in Vix

The `test_runner` module parses output into `TestResult { name, status, location }`
(unit tested) with a `tally` helper. The host runs the command with capture
(`run_tests` sets `test_capture`; `poll_command` buffers lines and calls
`finish_test_run` on exit), renders the panel with `ui::draw_test_panel`, and
routes row clicks to `jump_to_test`.
