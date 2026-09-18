; Definitions

(function_declaration
  name: (identifier) @name) @definition.function

(function_declaration
  name: (dot_index_expression
    table: (_) @qualifier
    field: (identifier) @name)) @definition.function

(function_declaration
  name: (method_index_expression
    table: (_) @qualifier
    method: (identifier) @name)) @definition.method

(assignment_statement
  (variable_list
    .
    name: (identifier) @name)
  (expression_list
    .
    value: (function_definition))) @definition.function

(assignment_statement
  (variable_list
    .
    name: (dot_index_expression
      table: (_) @qualifier
      field: (identifier) @name))
  (expression_list
    .
    value: (function_definition))) @definition.function

(table_constructor
  (field
    name: (identifier) @name
    value: (function_definition))) @definition.function

(assignment_statement
  (variable_list
    .
    name: (identifier) @qualifier)
  (expression_list
    .
    value: (table_constructor
      (field
        name: (identifier) @name
        value: (function_definition))))) @definition.function

; Calls

(function_call
  name: (identifier) @name) @reference.call

(function_call
  name: (dot_index_expression
    table: (_) @qualifier
    field: (identifier) @name)) @reference.call

(function_call
  name: (method_index_expression
    table: (_) @qualifier
    method: (identifier) @name)) @reference.call

; Imports

(function_call
  name: (identifier) @_require
  arguments: (arguments (string content: (string_content) @name))
  (#eq? @_require "require")) @reference.import

; Other mentions

(dot_index_expression
  table: (_) @qualifier
  field: (identifier) @name) @reference.field
