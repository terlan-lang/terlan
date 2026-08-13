/**
 * Tree-sitter grammar scaffold for Terlan.
 *
 * Inputs:
 * - Terlan source files using `.terl`, `.terls`, or `.terli`.
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
    [$.expression, $._name_ref],
    [$.expression, $.pattern],
    [$.constructor_pattern, $._name_ref],
    [$.list_expression, $.list_pattern],
    [$.shape_pattern, $.tuple_pattern],
    [$.shape_pattern, $.list_pattern],
    [$.shape_list_pattern, $.list_pattern],
    [$.binary_layout_expression, $.binary_layout_pattern],
    [$.type_expression, $._type_name_ref]
  ],

  rules: {
    source_file: ($) => repeat($._top_level_item),

    _top_level_item: ($) =>
      choice(
        $.module_declaration,
        $.import_declaration,
        $.annotation,
        $.constant_declaration,
        $.const_function_declaration,
        $.type_declaration,
        $.shape_declaration,
        $.struct_declaration,
        $.trait_declaration,
        $.impl_declaration,
        $.template_declaration,
        $.template_element,
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
        optional(choice(
          seq("=", field("body", $.type_expression)),
          seq(
            ":",
            field("representation", $.type_expression),
            "=",
            sep1($.valued_union_arm, "|")
          )
        )),
        "."
      ),

    valued_union_arm: ($) =>
      seq(field("name", $.type_identifier), "=", field("value", $.expression)),

    constant_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        "const",
        field("name", $.type_identifier),
        ":",
        field("type", $.type_expression),
        "=",
        field("value", $.expression),
        "."
      ),

    const_function_declaration: ($) =>
      seq(
        optional($.pub_keyword),
        "const",
        field("name", $.identifier),
        $.parameters,
        ":",
        field("type", $.type_expression),
        "->",
        field("body", $.expression),
        "."
      ),

    shape_declaration: ($) =>
      prec(
        2,
        seq(
          optional($.pub_keyword),
          "shape",
          field("name", $.type_identifier),
          optional($.shape_parameters),
          "=",
          field("body", $.shape_pattern),
          optional($.shape_guard_clause),
          "."
        )
      ),

    shape_parameters: ($) => seq("(", optional(commaSep1($.identifier)), ")"),

    shape_guard_clause: ($) => seq("where", $.shape_guard_expression),

    shape_guard_expression: ($) =>
      choice(
        $.shape_guard_binary_expression,
        $.identifier,
        $.type_identifier,
        $.number,
        $.string,
        seq("(", $.shape_guard_expression, ")")
      ),

    shape_guard_binary_expression: ($) =>
      prec.left(
        1,
        seq(
          $.shape_guard_expression,
          choice(
            "+",
            "-",
            "*",
            "/",
            "==",
            "!=",
            ">=",
            "<=",
            ">",
            "<",
            "in",
            "..",
            "and",
            "or",
            "|>"
          ),
          $.shape_guard_expression
        )
      ),

    shape_pattern: ($) =>
      choice(
        $.shape_tuple_pattern,
        $.shape_list_pattern,
        $.shape_constructor_pattern,
        $.pattern
      ),

    shape_tuple_pattern: ($) => seq("{", optional(commaSep1($.shape_pattern)), "}"),

    shape_list_pattern: ($) => seq("[", optional(commaSep1($.shape_pattern)), "]"),

    shape_constructor_pattern: ($) =>
      seq($._name_ref, "(", optional(commaSep1($.shape_pattern)), ")"),

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
        repeat(choice($.associated_constant_declaration, $.function_signature, $.function_declaration)),
        "}",
        "."
      ),

    impl_declaration: ($) =>
      choice(
        seq(
          optional($.pub_keyword),
          "impl",
          "not",
          field("trait", $._name_ref),
          "[",
          field("target", $.type_expression),
          "]",
          "."
        ),
        seq(
          optional($.pub_keyword),
          "impl",
          field("trait", $.impl_trait_ref),
          "for",
          field("target", $.type_expression),
          "{",
          repeat(choice($.impl_constant_declaration, $.function_declaration)),
          "}",
          "."
        )
      ),

    associated_constant_declaration: ($) =>
      seq("const", field("name", $.type_identifier), ":", $.type_expression, optional(seq("=", $.expression)), "."),

    impl_constant_declaration: ($) =>
      seq("const", field("name", $.type_identifier), optional(seq(":", $.type_expression)), "=", $.expression, "."),

    impl_trait_ref: ($) =>
      seq(
        $._type_name_ref,
        optional(
          seq(
            "[",
            commaSep1(choice($.type_expression, $.implication_type_parameter)),
            "]"
          )
        )
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
        optional($.guard_clause),
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
      choice(
        seq(field("name", $.identifier), ":", $.type_expression, optional(seq("=", $.expression))),
        seq(field("pattern", $.parameter_pattern), ":", $.type_expression),
        $.pattern
      ),

    field_declaration: ($) =>
      seq(field("name", $._field_identifier), ":", $.type_expression, optional(",")),

    config_declaration: ($) =>
      seq(field("name", $.identifier), "{", repeat(/[^}]/), "}", "."),

    type_parameters: ($) => seq("[", commaSep1($._type_parameter), "]"),

    _type_parameter: ($) =>
      choice($.type_identifier, $.const_type_parameter, $.implication_type_parameter),

    const_type_parameter: ($) =>
      seq("const", field("name", $.type_identifier), ":", field("kind", choice("Int", "Bool", "Atom"))),

    implication_type_parameter: ($) =>
      seq($.type_identifier, "=>", $.structural_evidence_shape),

    structural_evidence_shape: ($) =>
      seq("{", commaSep1($.implication_field), "}"),

    implication_field: ($) =>
      seq(field("name", $.identifier), ":", field("type", $._implication_type)),

    _implication_type: ($) =>
      choice($.structural_evidence_shape, $.type_expression),

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
        $.binary_layout_expression,
        $.list_expression,
        $.method_call_expression,
        $.call_expression,
        $.field_expression,
        $.cast_expression,
        $.unary_expression,
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

    let_expression: ($) =>
      choice(
        prec.dynamic(2, $.refutable_let_expression),
        prec.dynamic(
          1,
          prec.right(3, seq(repeat1(seq("let", $.let_binding)), $.expression))
        )
      ),

    let_binding: ($) => seq(field("pattern", $.pattern), "=", $.expression, ";"),

    refutable_let_expression: ($) =>
      prec.right(
        4,
        seq(
          "let",
          choice(
            field("binding", $.refutable_let_binding),
            seq(
              "{",
              field("binding", $.refutable_let_binding),
              repeat(seq(";", field("binding", $.refutable_let_binding))),
              optional(";"),
              "}"
            )
          ),
          "else",
          "{",
          repeat1($.case_arm),
          "}",
          ";",
          field("body", $.expression)
        )
      ),

    refutable_let_binding: ($) =>
      seq(field("pattern", $.pattern), "<-", field("value", $.expression)),

    case_expression: ($) =>
      seq("case", $.expression, "{", repeat1($.case_arm), "}"),

    case_arm: ($) => seq($.pattern, optional($.guard_clause), "->", $.expression, optional(";")),

    guard_clause: ($) => seq("where", $.expression),

    if_expression: ($) => seq("if", "{", repeat1($.case_arm), "}"),

    lambda_expression: ($) => seq($.parameters, "->", $.expression),

    binary_layout_expression: ($) =>
      seq(
        "Binary",
        "[",
        field("endian", $.binary_layout_endian),
        "]",
        "{",
        commaSep1($.binary_layout_field),
        "}"
      ),

    binary_layout_field: ($) =>
      seq(
        field("name", $.identifier),
        ":",
        field("descriptor", $.binary_layout_descriptor)
      ),

    list_expression: ($) =>
      seq(
        "[",
        optional(choice($.list_comprehension_body, commaSep1($.expression))),
        "]"
      ),

    list_comprehension_body: ($) =>
      seq(
        field("yield", $.expression),
        "|",
        commaSepNoTrailing($.comprehension_clause)
      ),

    comprehension_clause: ($) =>
      choice($.comprehension_generator, $.comprehension_filter),

    comprehension_generator: ($) =>
      seq(field("pattern", $.pattern), "<-", field("source", $.expression)),

    comprehension_filter: ($) => field("filter", $.expression),

    call_expression: ($) => seq($._name_ref, $.arguments),

    method_call_expression: ($) =>
      prec.left(5, seq($.expression, $._field_selector, $.arguments)),

    field_expression: ($) => prec.left(4, seq($.expression, $._field_selector)),

    cast_expression: ($) => prec.left(3, seq($.expression, "as", $.type_expression)),

    unary_expression: ($) => prec(4, seq(choice("-", "not"), $.expression)),

    binary_expression: ($) =>
      prec.left(
        1,
        seq(
          $.expression,
          choice(
            "+",
            "-",
            "*",
            "/",
            "div",
            "rem",
            "==",
            "!=",
            ">=",
            "<=",
            ">",
            "<",
            "in",
            "..",
            "and",
            "or",
            "|>"
          ),
          $.expression
        )
      ),

    raw_macro_expression: ($) => seq($.identifier, "{", repeat(/[^}]/), "}"),

    arguments: ($) => seq("(", optional(commaSep1($.argument)), ")"),

    argument: ($) => choice($.expression, seq($.identifier, "=", $.expression)),

    pattern: ($) =>
      choice(
        $.string_pattern,
        $.tuple_pattern,
        $.list_pattern,
        $.map_pattern,
        $.record_pattern,
        $.binary_layout_pattern,
        $.constructor_pattern,
        $.identifier,
        $.type_identifier,
        $.private_field_identifier,
        $.atom_literal,
        "_",
        $.number,
        $.string
      ),

    parameter_pattern: ($) =>
      choice(
        $.string_pattern,
        $.tuple_pattern,
        $.list_pattern,
        $.map_pattern,
        $.record_pattern,
        $.binary_layout_pattern,
        $.constructor_pattern,
        $.private_field_identifier,
        $.atom_literal,
        "_",
        $.number,
        $.string
      ),

    tuple_pattern: ($) => seq("{", commaSep1($.pattern), "}"),

    list_pattern: ($) =>
      seq(
        "[",
        optional(choice(seq($.pattern, "|", $.pattern), commaSep1($.pattern))),
        "]"
      ),

    map_pattern: ($) => seq("{", commaSep1($.map_pattern_field), "}"),

    map_pattern_field: ($) => seq(field("name", $.identifier), ":", field("value", $.pattern)),

    record_pattern: ($) =>
      seq($.type_identifier, "{", optional(commaSep1($.map_pattern_field)), "}"),

    binary_layout_pattern: ($) =>
      seq(
        "Binary",
        "[",
        field("endian", $.binary_layout_endian),
        "]",
        "{",
        commaSep1($.binary_layout_field),
        "}"
      ),

    binary_layout_endian: () => choice("big", "little"),

    binary_layout_descriptor: ($) =>
      choice(
        seq(
          field("kind", $.binary_layout_descriptor_kind),
          "[",
          field("width", $.binary_layout_width),
          "]"
        ),
        choice("Utf8", "Rest")
      ),

    binary_layout_descriptor_kind: () => choice("UInt", "IntBits", "Bytes", "Bits"),

    binary_layout_width: () => /[0-9]+/,

    constructor_pattern: ($) => seq($.type_identifier, "(", optional(commaSep1($.pattern)), ")"),

    interpolation: ($) =>
      seq(
        field("open", $.interpolation_start),
        field("content", $.expression),
        field("close", $.interpolation_end)
      ),

    interpolation_start: () => "${",

    interpolation_end: () => "}",

    template_element: ($) =>
      choice(
        seq(
          "<",
          field("open_tag", $.template_tag_name),
          repeat($.template_attribute),
          ">",
          repeat(choice($.template_element, $.template_text_interpolation, $.template_text)),
          "</",
          field("close_tag", $.template_tag_name),
          ">"
        ),
        seq(
          "<",
          field("tag", $.template_tag_name),
          repeat($.template_attribute),
          "/>"
        )
      ),

    template_tag_name: () => /[a-z][A-Za-z0-9-]*/,

    template_attribute: ($) =>
      seq(
        field("name", $.template_attribute_name),
        optional(seq("=", field("value", $.template_attribute_value)))
      ),

    template_attribute_name: () => /[a-z_:][A-Za-z0-9_:.\-]*/,

    template_attribute_value: ($) =>
      choice(
        seq(
          '"',
          repeat(choice($.template_attribute_interpolation, $.template_attribute_text)),
          '"'
        ),
        $.template_attribute_interpolation
      ),

    template_attribute_text: () => token.immediate(/[^"${]+/),

    template_attribute_interpolation: ($) =>
      seq(
        field("open", $.template_interpolation_start),
        field("content", $.expression),
        field("close", $.interpolation_end)
      ),

    template_text_interpolation: ($) =>
      seq(
        field("open", $.template_interpolation_start),
        field("content", $.expression),
        field("close", $.interpolation_end)
      ),

    template_interpolation_start: () => choice("${", "{"),

    template_text: () => token(prec(-1, /[^<${]+/)),

    string_pattern: () =>
      token(prec(1, /"([^"\\]|\\.)*\$\{[a-z_][A-Za-z0-9_]*(\s*:\s*[^}"\\]+)?\}([^"\\]|\\.)*"/)),

    atom_literal: ($) => seq("Atom", "[", $.string, "]"),

    qualified_identifier: () =>
      token(
        choice(
          /([a-z_][A-Za-z0-9_]*\.)+[A-Z][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*/,
          /[A-Z][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)+/
        )
      ),

    _name_ref: ($) => choice($.qualified_identifier, $.identifier, $.type_identifier),

    _type_name_ref: ($) => choice($.qualified_identifier, $.type_identifier),

    _field_identifier: ($) => choice($.identifier, $.private_field_identifier),

    _field_selector: ($) => choice($.field_identifier, $.private_field_selector),

    field_identifier: () => token.immediate(/\.[a-z_][A-Za-z0-9_]*/),

    private_field_selector: () => token.immediate(/\.#[a-z_][A-Za-z0-9_]*/),

    private_field_identifier: ($) => seq("#", $.identifier),

    pub_keyword: () => token(prec(1, "pub")),

    identifier: () => token(prec(-1, /[a-z_][A-Za-z0-9_]*/)),

    type_identifier: () => /[A-Z][A-Za-z0-9_]*/,

    number: () => /[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?/,

    string: () => token(prec(-1, /"([^"\\]|\\.)*"/)),

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

/** Builds a one-or-more separated production without a trailing separator. */
function sep1(rule, separator) {
  return seq(rule, repeat(seq(separator, rule)));
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
