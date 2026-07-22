[
  "<-"
  "|"
  "|>"
  "->"
  "+"
  "-"
  "*"
  "/"
  "=="
  "!="
  ">"
  ">="
  "<"
  "<="
  ".."
] @operator

[
  "case"
  "if"
  "in"
  "let"
  "when"
  "where"
] @keyword.control

[
  "constructor"
  "const"
  "implements"
  "import"
  "module"
  "opaque"
  "shape"
  "struct"
  "template"
  "trait"
  "type"
] @keyword

(constant_declaration name: (type_identifier) @constant)
(valued_union_arm name: (type_identifier) @constant)
(associated_constant_declaration name: (type_identifier) @constant)
(impl_constant_declaration name: (type_identifier) @constant)
(const_function_declaration name: (identifier) @function)

(pub_keyword) @keyword

(annotation "@" @punctuation.special name: (_) @attribute)

(module_declaration name: (qualified_identifier) @namespace)

(import_declaration path: (qualified_identifier) @namespace)

(function_declaration name: (identifier) @function)

(function_signature name: (identifier) @function)

(shape_declaration name: (type_identifier) @type)

(shape_guard_expression (identifier) @variable)

(template_declaration name: (type_identifier) @function)

(call_expression (qualified_identifier) @function.call)

(method_call_expression (field_identifier) @function.method)

(type_identifier) @type

(atom_type "Atom" @type.builtin)

(binary_layout_expression "Binary" @type.builtin)
(binary_layout_pattern "Binary" @type.builtin)
(binary_layout_descriptor_kind) @type.builtin
(binary_layout_descriptor "Utf8" @type.builtin)
(binary_layout_descriptor "Rest" @type.builtin)
(binary_layout_endian) @constant.builtin
(binary_layout_width) @number

(identifier) @variable

(field_declaration name: (identifier) @property)

(field_declaration name: (private_field_identifier) @property)

(field_expression (field_identifier) @property)

(field_expression (private_field_selector) @property)

(private_field_identifier "#" @punctuation.special)

(parameter name: (identifier) @variable.parameter)

(template_parameter name: (identifier) @variable.parameter)

(receiver name: (identifier) @variable.parameter)

(number) @number

(string) @string

(string_pattern) @string.special

(line_comment) @comment

(block_comment) @comment

(interpolation_start) @punctuation.special
(template_interpolation_start) @punctuation.special
(interpolation_end) @punctuation.special

(interpolation (expression) @embedded)
(template_text_interpolation (expression) @embedded)
(template_attribute_interpolation (expression) @embedded)
