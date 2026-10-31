; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(function_definition
  body: (_) @function.inside) @function.around

(struct_specifier
  body: (_) @class.inside) @class.around

(union_specifier
  body: (_) @class.inside) @class.around

(enum_specifier
  body: (_) @class.inside) @class.around

(parameter_list
  (_) @parameter.inside)

(argument_list
  (_) @parameter.inside)
