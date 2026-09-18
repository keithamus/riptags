; Definitions

(function_definition name: (word) @name) @definition.function
(variable_assignment name: (variable_name) @name) @definition.variable
(variable_assignment name: (subscript name: (variable_name) @name)) @definition.variable

; `local result` / `declare -a arr` declares a name without assigning it.
(declaration_command (variable_name) @name) @definition.variable

; Calls
;
; Shell builtins are never the symbol someone searches for, so the noisiest
; ones are filtered out: what is worth finding is a call to a function this
; tree defines.

(command
  name: (command_name (word) @name)
  (#not-any-of? @name
    "." "[" "builtin" "cd" "command" "declare" "echo" "eval" "exec" "exit"
    "export" "false" "local" "printf" "read" "readonly" "return" "set"
    "shift" "shopt" "source" "test" "trap" "true" "unset")) @reference.call

; Imports

; Only the first argument is the sourced file: `source lib.sh "$@"` passes the
; rest on.
(command
  name: (command_name (word) @_cmd)
  .
  argument: (word) @name
  (#any-of? @_cmd "source" ".")) @reference.import

; Other mentions

(simple_expansion (variable_name) @name) @reference.field
(expansion (variable_name) @name) @reference.field
(expansion (subscript name: (variable_name) @name)) @reference.field
