; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(function_item
  body: (_) @function.inside) @function.around

(closure_expression
  body: (_) @function.inside) @function.around

(struct_item
  body: (_) @class.inside) @class.around

(enum_item
  body: (_) @class.inside) @class.around

(union_item
  body: (_) @class.inside) @class.around

(trait_item
  body: (_) @class.inside) @class.around

(impl_item
  body: (_) @class.inside) @class.around

(parameters
  (_) @parameter.inside)

(closure_parameters
  (_) @parameter.inside)

(type_parameters
  (_) @parameter.inside)

(type_arguments
  (_) @parameter.inside)

(arguments
  (_) @parameter.inside)
