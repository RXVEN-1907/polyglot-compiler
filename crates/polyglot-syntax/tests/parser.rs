use polyglot_core::DiagnosticEngine;

fn compile_unit(src: &str) -> polyglot_syntax::TranslationUnit {
    let mut diags = DiagnosticEngine::new();
    let (_, unit) = polyglot_syntax::parse_source("test.c", src, &[], &mut diags)
        .expect("parse should not crash");
    if diags.has_errors() {
        // Rendering needs a SourceFile; rebuild minimal one.
        panic!("unexpected diagnostics: {}", diags.diagnostics().len());
    }
    unit
}

#[test]
fn parses_hello() {
    let unit = compile_unit(
        r#"
#include <stdio.h>
int main() {
    printf("Hello %d\n", 42);
    return 0;
}
"#,
    );
    assert!(unit.funcs.iter().any(|f| f.name == "printf" && f.variadic));
    let main = unit
        .funcs
        .iter()
        .find(|f| f.name == "main")
        .expect("main present");
    assert_eq!(main.params.len(), 0);
    assert_eq!(main.body.as_ref().unwrap().len(), 2);
}

#[test]
fn parses_macros_and_conditionals() {
    let unit = compile_unit(
        r#"
#define N 10
#define SQUARE(x) ((x) * (x))
#ifdef DEBUG
int debug_var = 1;
#else
int release_var = 2;
#endif
int main() {
    int x = SQUARE(N);
    return x - 100;
}
"#,
    );
    assert_eq!(unit.globals.len(), 1);
    assert_eq!(unit.globals[0].name, "release_var");
}

#[test]
fn parses_structs_typedefs() {
    let unit = compile_unit(
        r#"
typedef struct Point { int x; int y; } Point;
Point make(int x, int y) {
    Point p;
    p.x = x;
    p.y = y;
    return p; /* unsupported at sema level but must parse */
}
"#,
    );
    assert_eq!(unit.structs.len(), 1);
    assert_eq!(unit.structs[0].fields.len(), 2);
}

#[test]
fn parses_switch_and_loops() {
    let unit = compile_unit(
        r#"
int classify(int v) {
    switch (v) {
        case 0:
        case 1: return 1;
        default: return 0;
    }
}
int sum(int n) {
    int s = 0;
    for (int i = 1; i <= n; i++) s += i;
    while (s > 10) { s -= 10; }
    do { s++; } while (s < 3);
    return s;
}
"#,
    );
    assert_eq!(unit.funcs.len(), 2);
}
