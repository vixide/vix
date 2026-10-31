; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(element
  (start_tag)
  (_)+ @class.inside
  (end_tag)) @class.around

(element
  (self_closing_tag)) @class.around

(start_tag
  (attribute) @parameter.inside)

(self_closing_tag
  (attribute) @parameter.inside)
