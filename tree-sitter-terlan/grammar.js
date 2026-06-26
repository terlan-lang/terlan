/**
 * Tree-sitter grammar scaffold for Terlan.
 *
 * Inputs:
 * - Terlan source files using `.terl` or `.terli`.
 *
 * Outputs:
 * - A syntax tree that recognizes module/import headers, declarations,
 *   annotations, expressions, comments, strings, and interpolation islands.
 *
 * Transformation:
 * - Provides editor-oriented structure only. The compiler grammar remains the
 *   source of truth for parsing, validation, and diagnostics.
 */
module.exports = grammar({
  name: "terlan",

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    [$._top_level_item, $.function_declaration],
    [$.expression, $.raw_macro_expression],
    [$.expression, $._name_ref]
  ],

  rules: {
    source_file: ($) => repeat($._top_level_item),

    _top_level_item: ($) =>
      choice(
        $.module_declaration,
        $.import_declaration,
        $.annotation,
        $.type_declaration,
        $.struct_declaration,
        $.trait_declaration,
        $.impl_declaration,
        $.template_declaration,
        $.interpolation,
        $.function_declaration,
        $.constructor_declaration,
        $.config_declaration
      ),

    module_declaration: ($) =>
      seq("module", field("name", $.qualified_identifier), "."),

    import_declaration: ($) =>
      seq(
        "import",
        optional("type"),
        field("path", $.qualified_identifier),
        optional(seq(".", $.import_selection)),
        "."
      ),

    import_selection: ($) =>
      seq("{", commaSep1(choice($.identifier, $.type_identifier)), "}"),

    annotation: ($) => seq("@", field("name", $._name_ref), optional($.annotation_body)),

    annotation_body: ($) => seq("{", repeat($._annotation_item), "}"),

    _annotation_item: ($) =>
      choice($.annotation_assignment, $.annotation_section, $.expression),

    annotation_assignment: ($) =>
      seq(field("name", $.identifier), choice("=", ":"), field("value", $.expression)),

    annotation_section: ($) =>
      seq(field("name", $.identifier), "=", "{", repeat($._annotation_item), "}"),

    type_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        optional("opaque"),
        "type",
        field("name", $.type_identifier),
        optional($.type_parameters),
        "=",
        field("body", $.type_expression),
        "."
      ),

    struct_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        "struct",
        field("name", $.type_identifier),
        optional($.type_parameters),
        optional($.implements_clause),
        "{",
        repeat($.field_declaration),
        "}",
        "."
      ),

    implements_clause: ($) =>
      prec.right(seq("implements", commaSepNoTrailing($.type_expression))),

    trait_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        "trait",
        field("name", $.type_identifier),
        optional($.type_parameters),
        "{",
        repeat(choice($.function_signature, $.function_declaration)),
        "}",
        "."
      ),

    impl_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        "impl",
        field("trait", $.type_expression),
        "for",
        field("target", $.type_expression),
        "{",
        repeat($.function_declaration),
        "}",
        "."
      ),

    template_declaration: ($) =>
      seq(
        "template",
        field("name", $.type_identifier),
        "from",
        field("source", $.string),
        "{",
        repeat($.template_parameter),
        "}",
        "."
      ),

    template_parameter: ($) =>
      seq(field("name", $.identifier), ":", $.type_expression, optional(",")),

    constructor_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        "constructor",
        field("name", $.type_identifier),
        optional($.type_parameters),
        "{",
        repeat($.function_declaration),
        "}",
        "."
      ),

    function_declaration: ($) =>
      prec.right(
        1,
        seq(
          repeat($.annotation),
          optional($.pub_keyword),
          optional($.receiver),
          field("name", $.identifier),
          optional($.type_parameters),
          $.parameters,
          optional(seq(":", $.type_expression)),
          "->",
          $.expression,
          "."
        )
      ),

    function_signature: ($) =>
      seq(
        field("name", $.identifier),
        optional($.type_parameters),
        $.parameters,
        optional(seq(":", $.type_expression)),
        "."
      ),

    receiver: ($) =>
      seq("(", optional("mut"), field("name", $.identifier), ":", $.type_expression, ")"),

    parameters: ($) => seq("(", optional(commaSep1($.parameter)), ")"),

    parameter: ($) =>
      seq(field("name", $.identifier), ":", $.type_expression, optional(seq("=", $.expression))),

    field_declaration: ($) =>
      seq(field("name", $._field_identifier), ":", $.type_expression, optional(",")),

    config_declaration: ($) =>
      seq(field("name", $.identifier), "{", repeat(/[^}]/), "}", "."),

    type_parameters: ($) => seq("[", commaSep1($.type_identifier), "]"),

    type_expression: ($) =>
      choice(
        $.type_identifier,
        $.qualified_identifier,
        $.generic_type,
        $.tuple_type,
        $.function_type,
        $.atom_type
      ),

    generic_type: ($) => seq($._type_name_ref, "[", commaSep1($.type_expression), "]"),

    tuple_type: ($) => seq("{", optional(commaSep1($.type_expression)), "}"),

    function_type: ($) => seq("(", optional(commaSep1($.type_expression)), ")", "->", $.type_expression),

    atom_type: ($) => seq("Atom", "[", $.string, "]"),

    expression: ($) =>
      choice(
        $.let_expression,
        $.case_expression,
        $.if_expression,
        $.lambda_expression,
        $.method_call_expression,
        $.call_expression,
        $.field_expression,
        $.binary_expression,
        $.raw_macro_expression,
        $.interpolation,
        $.identifier,
        $.type_identifier,
        $.number,
        $.string,
        $.atom_literal,
        seq("(", $.expression, ")")
      ),

    let_expression: ($) => seq("let", repeat1($.let_binding), $.expression),

    let_binding: ($) => seq(field("name", $.identifier), "=", $.expression, ";"),

    case_expression: ($) =>
      seq("case", $.expression, "{", repeat1($.case_arm), "}"),

    case_arm: ($) => seq($.pattern, "->", $.expression, optional(";")),

    if_expression: ($) => seq("if", "{", repeat1($.case_arm), "}"),

    lambda_expression: ($) => seq($.parameters, "->", $.expression),

    call_expression: ($) => seq($._name_ref, $.arguments),

    method_call_expression: ($) =>
      prec.left(4, seq($.expression, ".", $._field_identifier, $.arguments)),

    field_expression: ($) => prec.left(3, seq($.expression, ".", $._field_identifier)),

    binary_expression: ($) =>
      prec.left(1, seq($.expression, choice("+", "-", "*", "/", "==", "!=", "|>"), $.expression)),

    raw_macro_expression: ($) => seq($.identifier, "{", repeat(/[^}]/), "}"),

    arguments: ($) => seq("(", optional(commaSep1($.argument)), ")"),

    argument: ($) => choice($.expression, seq($.identifier, "=", $.expression)),

    pattern: ($) =>
      choice($.identifier, $.type_identifier, $.private_field_identifier, $.atom_literal, "_", $.number, $.string),

    interpolation: ($) => seq("${", $.expression, "}"),

    atom_literal: ($) => seq("Atom", "[", $.string, "]"),

    qualified_identifier: () =>
      token(/([a-z_][A-Za-z0-9_]*\.)*[A-Z][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*/),

    _name_ref: ($) => choice($.qualified_identifier, $.identifier, $.type_identifier),

    _type_name_ref: ($) => choice($.qualified_identifier, $.type_identifier),

    _field_identifier: ($) => choice($.identifier, $.private_field_identifier),

    private_field_identifier: ($) => seq("#", $.identifier),

    pub_keyword: () => token(prec(1, "pub")),

    identifier: () => token(prec(-1, /[a-z_][A-Za-z0-9_]*/)),

    type_identifier: () => /[A-Z][A-Za-z0-9_]*/,

    number: () => /[0-9]+(\.[0-9]+)?/,

    string: () => /"([^"\\]|\\.)*"/,

    line_comment: () => token(seq("//", /.*/)),

    block_comment: () => token(seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/"))
  }
});

/**
 * Builds a comma-separated production.
 *
 * Inputs:
 * - `rule`: grammar rule accepted at each comma-separated position.
 *
 * Outputs:
 * - Tree-sitter rule matching one or more comma-separated values.
 *
 * Transformation:
 * - Reuses the common separator shape across imports, params, args, and types.
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)), optional(","));
}

/**
 * Builds a comma-separated production without a trailing comma.
 *
 * Inputs:
 * - `rule`: grammar rule accepted at each comma-separated position.
 *
 * Outputs:
 * - Tree-sitter rule matching one or more comma-separated values.
 *
 * Transformation:
 * - Keeps clauses followed by `{` unambiguous when a comma would otherwise
 *   make the parser expect another item.
 */
function commaSepNoTrailing(rule) {
  return seq(rule, repeat(seq(",", rule)));
}
