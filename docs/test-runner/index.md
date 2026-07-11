# Test Runner

Run your test suite and see a tidy pass/fail list instead of scrolling raw
output. **Tools → Run Tests** runs your test command, and the **Test panel**
shows each test with a ✓ / ✗ / ○ and lets you jump to failures.

## Configure

Set the command for your project in `config.toml`:

```toml
test_command = "cargo test"   # or "pytest -v", "npm test", "go test ./..."
```

## Using it

- **Tools → Run Tests** (or the command palette) runs `test_command`. Output still
  streams to the bottom dock; when it finishes, the **Test panel** fills with the
  parsed results and a notification reports how many passed / failed / were
  ignored.
- **Toggle Test Panel** shows or hides the panel on the right.
- Each row shows the test name with a status icon. **Click** a failing test to
  jump straight to the line that panicked (for `cargo test`; other runners show
  status without a location).

Vix™ understands `cargo test` (libtest) and the common `name PASSED/FAILED/SKIPPED`
format used by `pytest -v` and others.

See the specification at `crates/vix-test-runner/spec/index.md`. For arbitrary project
commands (not just tests) see [Tasks](../tasks/index.md).

---

Vix™ and Vix IDE™ are trademarks.
