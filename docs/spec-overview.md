<!-- mdBook page: transcludes the repo-root spec index rather than linking
     it directly, so `mdbook build` writes rendered output only under
     `book/` (a chapter path that escapes `src` writes its HTML next to
     the source file instead — see book.toml's own comment, and T301 in
     `tasks.md`). -->

{{#include ../spec/index/index.md}}
