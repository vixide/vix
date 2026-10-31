; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(function_declaration
  body: (block) @function.inside) @function.around

(method_declaration
  body: (block) @function.inside) @function.around

(func_literal
  body: (block) @function.inside) @function.around

(type_declaration
  (type_spec
    type: (struct_type) @class.inside)) @class.around

(type_declaration
  (type_spec
    type: (interface_type) @class.inside)) @class.around

(parameter_list
  (_) @parameter.inside)

(type_parameter_list
  (_) @parameter.inside)

(argument_list
  (_) @parameter.inside)
