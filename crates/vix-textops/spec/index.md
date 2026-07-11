# Textops

Small pure text transforms used by Edit/Tools actions.

Two shapes live here: whole-text transforms (`&str -> String`: line-ending
conversion, blank-line squeezing, ROT13) and cursor-relative rewrites
(`(&str, usize) -> Option<(String, usize)>`: increment number, smart toggle,
transpose). The host applies the former via
`App::transform_selection_or_buffer` and the latter via
`App::rewrite_at_cursor`; everything here is unit-tested without a terminal.
