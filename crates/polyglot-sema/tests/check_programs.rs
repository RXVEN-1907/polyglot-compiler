//! Integration tests for semantic analysis (`polyglot-sema`).
//!
//! Well-typed programs must check cleanly; ill-typed programs must be
//! rejected with coded, spanned diagnostics. `sizeof` and constant
//! expressions must fold to concrete values.

use polyglot_core::{DiagnosticEngine, Type};
use polyglot_sema::{check, const_eval, CheckedProgram};
use polyglot_syntax::ast::{ExprKind, Init, Stmt, StmtKind};
use polyglot_syntax::lex::Suffix;

/// Parse + type-check `src`, panicking with the diagnostic codes on failure.
fn checked(src: &str) -> CheckedProgram {
    let mut diags = DiagnosticEngine::new();
    let (_, unit) = polyglot_syntax::parse_source("test.c", src, &[], &mut diags)
        .expect("parse should not crash");
    assert!(
        !diags.has_errors(),
        "unexpected parse errors: {} diagnostic(s)",
        diags.diagnostics().len()
    );
    match check(unit, &mut diags) {
        Ok(program) => program,
        Err(_) => {
            let msgs: Vec<String> = diags
                .diagnostics()
                .iter()
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect();
            panic!("unexpected type errors: {}", msgs.join("; "));
        }
    }
}

/// Parse + type-check `src`, expecting failure; returns the diagnostics
/// plus the preprocessed source (needed to resolve spans to logical lines).
fn check_errors(src: &str) -> (DiagnosticEngine, polyglot_core::SourceFile) {
    let mut diags = DiagnosticEngine::new();
    let (source, unit) = polyglot_syntax::parse_source("test.c", src, &[], &mut diags)
        .expect("parse should not crash");
    let _ = check(unit, &mut diags);
    assert!(diags.has_errors(), "expected type errors in:\n{src}");
    (diags, source)
}

fn has_code(diags: &DiagnosticEngine, code: &str) -> bool {
    diags.diagnostics().iter().any(|d| d.code == code)
}

fn body_of<'a>(program: &'a CheckedProgram, name: &str) -> &'a [Stmt] {
    program
        .unit
        .funcs
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function `{name}` present"))
        .body
        .as_deref()
        .unwrap_or_else(|| panic!("function `{name}` has a body"))
}

// ---- well-typed programs -------------------------------------------------

#[test]
fn well_typed_hello_passes() {
    let program = checked(
        r#"
#include <stdio.h>
int main() {
    printf("Hello %d\n", 42);
    return 0;
}
"#,
    );
    // `printf` is implicitly declared (C89 style) but only warns.
    assert!(program.funcs.contains_key("printf"));
    assert_eq!(
        program
            .unit
            .funcs
            .iter()
            .filter(|f| f.name == "main")
            .count(),
        1
    );
}

#[test]
fn well_typed_structs_and_pointers_pass() {
    let program = checked(
        r#"
struct Point { int x; int y; };
int dist_sq(struct Point *p) {
    return p->x * p->x + p->y * p->y;
}
int main() {
    struct Point pt;
    struct Point *pp = &pt;
    pt.x = 3;
    pp->y = 4;
    int arr[5];
    arr[0] = dist_sq(pp);
    return arr[0];
}
"#,
    );
    // Struct layout: two 4-byte fields, no padding needed.
    let point = program.structs.get("Point").expect("Point laid out");
    assert_eq!(point.size, 8);
    assert_eq!(point.align, 4);
    assert_eq!(point.fields[0].offset, 0);
    assert_eq!(point.fields[1].offset, 4);
    // Signature table records the definition.
    let sig = program.funcs.get("dist_sq").expect("dist_sq sig");
    assert_eq!(
        sig.params,
        vec![Type::ptr_to(Type::Struct("Point".to_string()))]
    );
    assert!(sig.defined);
}

// ---- type errors ----------------------------------------------------------

#[test]
fn rejects_undeclared_identifier() {
    let (diags, source) = check_errors("int main() { return nope; }");
    assert!(
        has_code(&diags, "E0325"),
        "want E0325, got {:?}",
        diags.diagnostics()
    );
    // The error is spanned, and resolves to the offending source line.
    let d = diags
        .diagnostics()
        .iter()
        .find(|d| d.code == "E0325")
        .unwrap();
    let label = d.label.as_ref().expect("error carries a label");
    let (path, line) = source.resolve_line(label.span.line);
    assert_eq!(path, std::path::Path::new("test.c"));
    assert_eq!(line, 1);
    assert_eq!(label.message, "here");
}

#[test]
fn rejects_wrong_argument_count() {
    let (diags, _source) = check_errors(
        r#"
int add(int a, int b) { return a + b; }
int main() { return add(1); }
"#,
    );
    assert!(has_code(&diags, "E0327"));
    let msg = diags
        .diagnostics()
        .iter()
        .find(|d| d.code == "E0327")
        .unwrap()
        .message
        .clone();
    assert!(msg.contains("expects 2 argument(s)"), "message was: {msg}");
}

#[test]
fn rejects_function_redefinition() {
    let (diags, source) = check_errors(
        r#"
int f() { return 1; }
int f() { return 2; }
"#,
    );
    assert!(has_code(&diags, "E0302"));
    // The redefinition diagnostic points at the second definition's line.
    let d = diags
        .diagnostics()
        .iter()
        .find(|d| d.code == "E0302")
        .unwrap();
    let (_, line) = source.resolve_line(d.label.as_ref().unwrap().span.line);
    assert_eq!(line, 3);
}

#[test]
fn rejects_global_redefinition() {
    let (diags, _source) = check_errors("int g = 1;\nint g = 2;\n");
    assert!(has_code(&diags, "E0304"));
}

#[test]
fn rejects_local_redeclaration_in_same_scope() {
    let (diags, _source) = check_errors("int main() { int x = 1; int x = 2; return x; }");
    assert!(has_code(&diags, "E0310"));
}

#[test]
fn rejects_struct_plus_int_arithmetic() {
    let (diags, _source) = check_errors(
        r#"
struct S { int a; };
int main() { struct S s; return s + 1; }
"#,
    );
    assert!(has_code(&diags, "E0346"));
}

#[test]
fn rejects_dereference_of_non_pointer() {
    let (diags, _source) = check_errors("int main() { int x = 5; return *x; }");
    assert!(has_code(&diags, "E0334"));
}

#[test]
fn rejects_void_function_returning_value() {
    let (diags, _source) = check_errors("void f() { return 3; }");
    assert!(has_code(&diags, "E0318"));
}

#[test]
fn rejects_break_outside_loop() {
    let (diags, _source) = check_errors("int main() { break; return 0; }");
    assert!(has_code(&diags, "E0311"));
}

#[test]
fn rejects_duplicate_case_values() {
    // `2 + 2` and `4` fold to the same constant via const_eval.
    let (diags, _source) = check_errors(
        r#"
int f(int v) {
    switch (v) { case 2 + 2: return 1; case 4: return 2; }
    return 0;
}
"#,
    );
    assert!(has_code(&diags, "E0315"));
    let msg = diags
        .diagnostics()
        .iter()
        .find(|d| d.code == "E0315")
        .unwrap()
        .message
        .clone();
    assert!(msg.contains('4'), "duplicate-value message was: {msg}");
}

// ---- sizeof / const-eval ---------------------------------------------------

#[test]
fn sizeof_folds_to_constants() {
    let program = checked(
        r#"
struct Point { int x; int y; };
int sizes() {
    int a = sizeof(int);
    long b = sizeof(long);
    int c = sizeof(char);
    int d = sizeof(struct Point);
    int arr[10];
    long e = sizeof(arr);
    return a + c + d;
}
"#,
    );
    let mut sizes: Vec<(String, i128)> = Vec::new();
    for stmt in body_of(&program, "sizes") {
        if let StmtKind::Decl {
            name,
            init: Some(Init::Expr(e)),
            ..
        } = &stmt.kind
        {
            if let ExprKind::IntLit(v, suffix) = &e.kind {
                assert!(
                    matches!(suffix, Suffix::Long),
                    "sizeof folds to a long literal"
                );
                sizes.push((name.clone(), *v));
            }
        }
    }
    let get = |name: &str| {
        sizes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
            .unwrap_or_else(|| panic!("decl `{name}` folded by sizeof"))
    };
    assert_eq!(get("a"), 4, "sizeof(int)");
    assert_eq!(get("b"), 8, "sizeof(long)");
    assert_eq!(get("c"), 1, "sizeof(char)");
    assert_eq!(get("d"), 8, "sizeof(struct Point) = 2 * int");
    assert_eq!(
        get("e"),
        40,
        "sizeof of an array does NOT decay to a pointer"
    );
}

#[test]
fn const_eval_evaluates_constant_expressions() {
    let mut diags = DiagnosticEngine::new();
    let (_, unit) = polyglot_syntax::parse_source(
        "test.c",
        "int main(int x) { return 6 * (2 + 5); }",
        &[],
        &mut diags,
    )
    .expect("parse ok");
    let ret = match &unit.funcs[0].body.as_ref().unwrap()[0].kind {
        StmtKind::Return(Some(e)) => e,
        other => panic!("expected a return statement, got {other:?}"),
    };
    assert_eq!(const_eval(ret), Some(42));
    // Non-constant operands make it None.
    let mut diags2 = DiagnosticEngine::new();
    let (_, unit2) = polyglot_syntax::parse_source(
        "test.c",
        "int main(int x) { return x + 1; }",
        &[],
        &mut diags2,
    )
    .expect("parse ok");
    let ret2 = match &unit2.funcs[0].body.as_ref().unwrap()[0].kind {
        StmtKind::Return(Some(e)) => e,
        other => panic!("expected a return statement, got {other:?}"),
    };
    assert_eq!(const_eval(ret2), None);
}

#[test]
fn array_length_resolved_from_constant_expression() {
    let program = checked(
        r#"
int main() {
    int a[2 + 3];
    a[0] = 7;
    return a[0];
}
"#,
    );
    let body = body_of(&program, "main");
    match &body[0].kind {
        StmtKind::Decl { ty, name, .. } => {
            assert_eq!(name, "a");
            assert_eq!(*ty, Type::Array(Box::new(Type::int()), Some(5)));
        }
        other => panic!("expected a declaration, got {other:?}"),
    }
}

#[test]
fn string_literal_initializer_sizes_char_array() {
    let program = checked(
        r#"
char msg[] = "hi";
int main() { return 0; }
"#,
    );
    // "hi" needs 3 bytes including the NUL terminator.
    assert_eq!(program.globals.len(), 1);
    assert_eq!(program.globals[0].name, "msg");
    assert_eq!(
        program.globals[0].ty,
        Type::Array(Box::new(Type::char_t()), Some(3))
    );
}

#[test]
fn overly_long_string_initializer_rejected() {
    let (diags, _source) = check_errors("int main() { char s[2] = \"abc\"; return 0; }");
    assert!(
        has_code(&diags, "E0320"),
        "string longer than array must error"
    );
}
