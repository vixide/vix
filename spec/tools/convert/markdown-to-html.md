# Convert from Markdown into HTML

Subcrate vix-convert-from-markdown-into-html-tool

- menu "Tools"
  - submenu "Convert"
    - submenu "Markdown"
      - menuitem "HTML"

Convert the selected text (or if no selected text then the entire buffer) from Markdown into HTML.

Use option for GitHub Flavored Markdown (GFM):

https://docs.rs/markdown/latest/markdown/fn.to_html_with_options.html

```rust
let result = to_html_with_options(str, &Options::gfm())?;
```
