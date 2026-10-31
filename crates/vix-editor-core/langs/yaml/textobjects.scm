; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(block_mapping_pair
  value: (_) @class.inside) @class.around

(block_mapping_pair) @parameter.inside
