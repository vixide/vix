# Regex Tool

A live regular-expression tester, plus the dialog's editing state.

The dialog has two fields — the pattern and the subject text — and shows the
matches (or the compile error) as the user types. Matching uses the same
`regex` engine as Find/Replace. The host renders the [`Tester`]; this module
holds the fields and computes [`Tester::result`].
