; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(function_definition
  body: (block) @function.inside) @function.around

(lambda
  body: (_) @function.inside) @function.around

(class_definition
  body: (block) @class.inside) @class.around

(parameters
  (_) @parameter.inside)

(lambda_parameters
  (_) @parameter.inside)

(argument_list
  (_) @parameter.inside)
