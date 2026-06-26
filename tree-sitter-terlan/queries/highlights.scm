[
  "case"
  "if"
  "let"
] @keyword.control

[
  "constructor"
  "implements"
  "import"
  "module"
  "opaque"
  "struct"
  "template"
  "trait"
  "type"
] @keyword

(pub_keyword) @keyword

(annotation "@" @punctuation.special name: (_) @attribute)

(module_declaration name: (qualified_identifier) @namespace)

(import_declaration path: (qualified_identifier) @namespace)

(function_declaration name: (identifier) @function)

(function_signature name: (identifier) @function)

(template_declaration name: (type_identifier) @function)

(call_expression (qualified_identifier) @function.call)

(method_call_expression (identifier) @function.method)

(type_identifier) @type

(atom_type "Atom" @type.builtin)

(identifier) @variable

(field_declaration name: (identifier) @property)

(field_declaration name: (private_field_identifier) @property)

(field_expression (private_field_identifier) @property)

(private_field_identifier "#" @punctuation.special)

(parameter name: (identifier) @variable.parameter)

(template_parameter name: (identifier) @variable.parameter)

(receiver name: (identifier) @variable.parameter)

(number) @number

(string) @string

(line_comment) @comment

(block_comment) @comment

(interpolation "${" @punctuation.special "}" @punctuation.special)

(interpolation (expression) @embedded)
