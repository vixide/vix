# Format Tool

Reformat the selection (or buffer) within one data format: pretty-print or
minify JSON, and canonicalize YAML and TOML.

Each function parses the text into a generic value and re-serializes it, so
the document is normalized (consistent indentation, key handling, quoting)
without changing its meaning. Used by the Tools → Format menu via
`App::transform_selection_or_buffer_try`.
