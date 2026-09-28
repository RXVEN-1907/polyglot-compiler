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

// ---- switch ----------------------------------------------------------------

use polyglot_syntax::ast::{ExprKind, StmtKind};

/// Collect the labels of every section in a function's first switch.
fn switch_sections(src: &str) -> Vec<polyglot_syntax::ast::SwitchSection> {
    let unit = compile_unit(src);
    let f = unit.funcs.iter().find(|f| f.name == "classify").unwrap();
    let mut found = Vec::new();
    for stmt in f.body.as_ref().unwrap() {
        if let StmtKind::Switch { sections, .. } = &stmt.kind {
            found = sections.clone();
        }
    }
    assert!(!found.is_empty(), "switch present in:\n{src}");
    found
}

#[test]
fn parses_switch_sections_labels_and_fallthrough() {
    let sections = switch_sections(
        r#"
int classify(int v) {
    switch (v) {
        case 'a':
        case 1: return 1;
        case -2 + 0: { v = 9; }
        default: return 0;
    }
}
"#,
    );
    assert_eq!(sections.len(), 3);
    // Section one carries two stacked labels: 'a' (97) and 1.
    let l0: Vec<_> = sections[0]
        .labels
        .iter()
        .map(|l| match l {
            Some(e) => match &e.kind {
                ExprKind::IntLit(v, _) => *v,
                other => panic!("integer case label, got {other:?}"),
            },
            None => panic!("unexpected default"),
        })
        .collect();
    assert_eq!(l0, vec![97, 1]);
    // A computed constant label (`case -2 + 0:`) opens its own section
    // with a non-literal constant expression (evaluated by sema).
    assert!(sections[1].labels.iter().any(|l| l.is_some()));
    assert!(sections[0]
        .stmts
        .iter()
        .any(|s| matches!(&s.kind, StmtKind::Return(_))));
    // `default` is the None label.
    assert!(sections
        .iter()
        .any(|s| s.labels.iter().any(|l| l.is_none())));
    // A section between labels keeps its statements (fallthrough is
    // decided at lowering, not parse time).
    assert!(sections[1]
        .stmts
        .iter()
        .any(|s| matches!(&s.kind, StmtKind::Block(_))));
}

// ---- enums -----------------------------------------------------------------

#[test]
fn parses_enums_with_implicit_and_explicit_values() {
    let unit = compile_unit(
        r#"
enum Color { RED, GREEN, BLUE = 10, NEXT };
typedef enum { OFF = 0, ON } Toggle;
enum Color pick(void) {
    enum Color c = GREEN;
    int limit = BLUE - RED;
    int flags[NEXT];
    switch (c) {
        case RED: return c;
        case BLUE: break;
    }
    return c;
}
int use_toggle(void) { return ON + 1; }
"#,
    );
    // Two enum definitions recorded (Color, and the anonymous typedef'd one).
    assert_eq!(unit.enums.len(), 2);
    let color = &unit.enums[0];
    assert_eq!(color.name, "Color");
    assert_eq!(
        color.constants,
        vec![
            ("RED".to_string(), 0),
            ("GREEN".to_string(), 1),
            ("BLUE".to_string(), 10),
            ("NEXT".to_string(), 11),
        ]
    );
    // Anonymous enums keep an empty tag.
    assert_eq!(unit.enums[1].name, "");
    // Constants fold to integer literals in expressions.
    let pick = unit.funcs.iter().find(|f| f.name == "pick").unwrap();
    let mut saw_init_const = false;
    for stmt in pick.body.as_ref().unwrap() {
        if let StmtKind::Decl {
            init: Some(polyglot_syntax::ast::Init::Expr(e)),
            ..
        } = &stmt.kind
        {
            if matches!(e.kind, ExprKind::IntLit(1, _)) {
                saw_init_const = true; // `= GREEN`
            }
        }
    }
    assert!(saw_init_const, "GREEN folds to IntLit(1)");
}

#[test]
fn parses_bare_enum_and_trailing_comma() {
    let unit = compile_unit(
        r#"
enum Lang { C, RUST, };
int main(void) { return RUST; }
"#,
    );
    assert_eq!(unit.enums.len(), 1);
    assert_eq!(unit.enums[0].constants.len(), 2);
}

#[test]
fn parses_bare_local_enum_and_struct_definitions() {
    let unit = compile_unit(
        r#"
int main(void) {
    enum Local { A = 1, B };
    struct Spot { int x; };
    int v = B;
    return v;
}
"#,
    );
    assert_eq!(unit.enums.len(), 1);
    assert_eq!(unit.enums[0].constants.len(), 2);
    assert_eq!(unit.structs.len(), 1);
}

// ---- unions ----------------------------------------------------------------

#[test]
fn parses_unions_with_overlapping_members() {
    let unit = compile_unit(
        r#"
union Value { int i; char bytes[8]; };
int read(union Value *v) {
    return v->i;
}
"#,
    );
    assert_eq!(unit.structs.len(), 1);
    let def = &unit.structs[0];
    assert_eq!(def.name, "Value");
    assert_eq!(def.fields.len(), 2);
    // Every union member overlaps at offset 0.
    assert_eq!(def.fields[0].name, "i");
    assert_eq!(def.fields[0].offset, 0);
    assert_eq!(def.fields[1].name, "bytes");
    assert_eq!(def.fields[1].offset, 0);
    // size = max member (8) rounded to align (4).
    assert_eq!(def.size, 8);
    assert_eq!(def.align, 4);
}

// ---- bit-fields ------------------------------------------------------------

#[test]
fn parses_distinct_anonymous_structs() {
    let unit = compile_unit(
        r#"
struct { int a; } first;
struct { char b; } second;
"#,
    );
    // Each anonymous definition gets its own unique internal tag.
    assert_eq!(unit.structs.len(), 2);
    let names: Vec<&str> = unit.structs.iter().map(|s| s.name.as_str()).collect();
    assert_ne!(names[0], names[1]);
    assert!(names.iter().all(|n| n.starts_with(".anon")));
}

#[test]
fn parses_bitfield_widths_and_padding() {
    let unit = compile_unit(
        r#"
struct Flags { unsigned a:3; unsigned b:5; int :0; unsigned c:4; };
struct Mixed { char t; int x:4; int y; };
int get(struct Flags *f) { return f->a; }
"#,
    );
    let flags = unit.structs.iter().find(|s| s.name == "Flags").unwrap();
    assert_eq!(flags.fields.len(), 4);
    assert_eq!(flags.fields[0].name, "a");
    let bf0 = flags.fields[0].bitfield.expect("a is a bit-field");
    assert_eq!(bf0.width, 3);
    assert!(!bf0.signed, "unsigned bit-field");
    let bf1 = flags.fields[1].bitfield.expect("b is a bit-field");
    assert_eq!(bf1.width, 5);
    // `int :0` is an unnamed zero-width boundary marker.
    assert_eq!(flags.fields[2].name, "");
    let bf2 = flags.fields[2].bitfield.expect(":0 marker");
    assert_eq!(bf2.width, 0);
    assert!(flags.fields[3].name == "c");
    // Named signed bit-field.
    let mixed = unit.structs.iter().find(|s| s.name == "Mixed").unwrap();
    let bf = mixed.fields[1].bitfield.expect("x is a bit-field");
    assert_eq!(bf.width, 4);
    assert!(bf.signed, "`int x:4` defaults to signed");
    // The int after the bit-field is byte-aligned at 4.
    assert_eq!(mixed.fields[2].offset, 4);
}
