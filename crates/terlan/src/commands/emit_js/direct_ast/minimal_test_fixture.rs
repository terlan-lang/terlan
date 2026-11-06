//! Minimal direct-Oxc fixture kept outside production lowering.

/// Builds and prints a minimal JavaScript module directly through Oxc AST APIs.
///
/// Inputs:
/// - None. The fixture shape is fixed to a small exported arithmetic function.
///
/// Output:
/// - JavaScript source printed by Oxc codegen.
///
/// Transformation:
/// - Constructs an Oxc `Program` with `AstBuilder`, then prints it through
///   `oxc_codegen`. This proves the direct AST construction path compiles
///   before production CoreIR lowering switches to Oxc AST nodes.
pub(crate) fn emit_minimal_direct_oxc_ast_module() -> String {
    use oxc_ast::{
        ast::{FormalParameterKind, FunctionType, ImportOrExportKind, Statement},
        AstBuilder, NONE,
    };
    use oxc_span::{SourceType, SPAN};
    use oxc_syntax::operator::BinaryOperator;

    let allocator = oxc_allocator::Allocator::default();
    let ast = AstBuilder::new(&allocator);

    let param_a = ast.formal_parameter(
        SPAN,
        ast.vec(),
        ast.binding_pattern_binding_identifier(SPAN, "A"),
        NONE,
        NONE,
        false,
        None,
        false,
        false,
    );
    let param_b = ast.formal_parameter(
        SPAN,
        ast.vec(),
        ast.binding_pattern_binding_identifier(SPAN, "B"),
        NONE,
        NONE,
        false,
        None,
        false,
        false,
    );
    let params = ast.alloc_formal_parameters(
        SPAN,
        FormalParameterKind::FormalParameter,
        ast.vec_from_array([param_a, param_b]),
        NONE,
    );
    let return_expr = ast.expression_binary(
        SPAN,
        ast.expression_identifier(SPAN, "A"),
        BinaryOperator::Addition,
        ast.expression_identifier(SPAN, "B"),
    );
    let return_stmt = ast.statement_return(SPAN, Some(return_expr));
    let body = ast.alloc_function_body(SPAN, ast.vec(), ast.vec1(return_stmt));
    let declaration = ast.declaration_function(
        SPAN,
        FunctionType::FunctionDeclaration,
        Some(ast.binding_identifier(SPAN, "add")),
        false,
        false,
        false,
        NONE,
        NONE,
        params,
        NONE,
        Some(body),
    );
    let export = ast.module_declaration_export_named_declaration(
        SPAN,
        Some(declaration),
        ast.vec(),
        None,
        ImportOrExportKind::Value,
        NONE,
    );
    let program = ast.program(
        SPAN,
        SourceType::mjs(),
        "",
        ast.vec(),
        None,
        ast.vec(),
        ast.vec1(Statement::from(export)),
    );
    oxc_codegen::Codegen::new().build(&program).code
}
