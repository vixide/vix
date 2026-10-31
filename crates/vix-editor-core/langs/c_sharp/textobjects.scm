; Textobjects (T616). Written for Vix against this grammar's node types;
; the capture names (@function.inside/.around, @class.*, @parameter.*)
; follow the convention Helix and nvim-treesitter-textobjects share, but
; no query text is copied from either. Provenance and the meaning of each
; capture: crates/vix-editor-core/spec/textobjects/index.md.

(method_declaration
  body: (_) @function.inside) @function.around

(constructor_declaration
  body: (_) @function.inside) @function.around

(destructor_declaration
  body: (_) @function.inside) @function.around

(local_function_statement
  body: (_) @function.inside) @function.around

(lambda_expression
  body: (_) @function.inside) @function.around

(class_declaration
  body: (_) @class.inside) @class.around

(struct_declaration
  body: (_) @class.inside) @class.around

(interface_declaration
  body: (_) @class.inside) @class.around

(enum_declaration
  body: (_) @class.inside) @class.around

(record_declaration
  body: (_) @class.inside) @class.around

(parameter_list
  (_) @parameter.inside)

(argument_list
  (_) @parameter.inside)
