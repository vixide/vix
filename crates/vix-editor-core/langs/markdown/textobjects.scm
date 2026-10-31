; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(section
  (atx_heading)
  (_)+ @class.inside) @class.around

(section
  (setext_heading)
  (_)+ @class.inside) @class.around
