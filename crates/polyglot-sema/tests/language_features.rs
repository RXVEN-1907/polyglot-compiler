//! Integration tests for the language features `polyglot-sema` checks
//! beyond the basics: `switch` labels, `enum` definitions, `union`
//! layout and struct/union bit-fields.
//!
//! Layout expectations match gcc and clang on ELF x86-64 (probed
//! directly); the one documented deviation is that the frontend's
//! integer model collapses `short` into 32-bit `int`, so bit-fields
//! declared `short` pack into 32-bit units.

use polyglot_core::{BitfieldLayout, DiagnosticEngine, Type};
use polyglot_sema::{check, CheckedProgram};
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

/// Parse + type-check `src`, expecting failure; returns the diagnostics.
fn check_errors(src: &str) -> DiagnosticEngine {
    let mut diags = DiagnosticEngine::new();
    let (_, unit) = polyglot_syntax::parse_source("test.c", src, &[], &mut diags)
        .expect("parse should not crash");
    let _ = check(unit, &mut diags);
    assert!(diags.has_errors(), "expected type errors in:\n{src}");
    diags
}

fn has_code(diags: &DiagnosticEngine, code: &str) -> bool {
    diags.diagnostics().iter().any(|d| d.code == code)
}

// ---- switch ---------------------------------------------------------------

#[test]
fn accepts_switch_labels_and_default() {
    let program = checked(
        r#"
int classify(int v) {
    switch (v) {
        case 0:
        case 'a': return 1;
        case 2 * 3: return 2;
        default: return 0;
    }
}
"#,
    );
    assert!(program.funcs.contains_key("classify"));
}

#[test]
fn rejects_multiple_default_labels() {
    let diags = check_errors(
        r#"
int f(int v) {
    switch (v) {
        case 1: return 1;
        default: return 0;
        default: return 2;
    }
}
"#,
    );
    assert!(has_code(&diags, "E0317"), "multiple `default` labels");
}

#[test]
fn rejects_non_constant_case_labels() {
    let diags = check_errors(
        r#"
int f(int v, int other) {
    switch (v) {
        case other: return 1;
    }
    return 0;
}
"#,
    );
    assert!(has_code(&diags, "E0316"), "case must be constant");
}

#[test]
fn rejects_non_integer_switch_scrutinee() {
    let diags = check_errors(
        r#"
struct S { int x; };
int f(struct S s) {
    switch (s) {
        case 1: return 1;
    }
    return 0;
}
"#,
    );
    assert!(has_code(&diags, "E0314"), "scrutinee must be an integer");
}

// ---- enums ----------------------------------------------------------------

#[test]
fn enum_constants_work_as_int_constants() {
    let program = checked(
        r#"
enum Color { RED, GREEN, BLUE = 10, NEXT };
int table(void) {
    int scores[NEXT];   // enumerator as array length (11)
    scores[BLUE] = GREEN;
    switch (scores[BLUE]) {
        case RED: return 1;
        case BLUE: return 2;
    }
    return 0;
}
"#,
    );
    // `int scores[NEXT]` resolves with the enumerator's value.
    let checked_body = program
        .unit
        .funcs
        .iter()
        .find(|f| f.name == "table")
        .and_then(|f| f.body.as_ref())
        .expect("table body");
    let arr_len = checked_body.iter().find_map(|s| match &s.kind {
        polyglot_syntax::ast::StmtKind::Decl {
            ty: Type::Array(_, Some(n)),
            ..
        } => Some(*n),
        _ => None,
    });
    assert_eq!(arr_len, Some(11), "NEXT folds to 11 for the array length");
}

#[test]
fn enum_typed_variables_behave_as_ints() {
    // Enums are ints: plain int values assign freely and arithmetic
    // works; the tag itself carries no checking.
    checked(
        r#"
enum Color { RED, GREEN };
int main(void) {
    enum Color c = 5;
    c = c + 1;
    int plain = c;
    return plain;
}
"#,
    );
}

#[test]
fn rejects_variable_named_like_an_enumerator() {
    let diags = check_errors(
        r#"
enum Color { RED, GREEN };
int main(void) {
    int GREEN = 3;
    return GREEN;
}
"#,
    );
    assert!(has_code(&diags, "E0349"), "enumerator name reuse");
    let diags = check_errors(
        r#"
enum Color { RED, GREEN };
int RED = 7;
"#,
    );
    assert!(has_code(&diags, "E0349"), "global enumerator name reuse");
}

// ---- unions ---------------------------------------------------------------

#[test]
fn union_members_overlap_and_size_is_the_largest() {
    let program = checked(
        r#"
union Value { int i; char bytes[8]; };
int pun(union Value *v) {
    v->i = 65;
    return v->bytes[0];
}
"#,
    );
    let def = program.structs.get("Value").expect("Value laid out");
    assert_eq!(def.size, 8, "largest member (char[8]) rounded to align 4");
    assert_eq!(def.align, 4);
    for f in &def.fields {
        assert_eq!(f.offset, 0, "union member `{}` overlaps at 0", f.name);
    }
}

#[test]
fn unions_nest_inside_structs_and_vice_versa() {
    let program = checked(
        r#"
struct Inner { char tag; int payload; };
union Outer { struct Inner s; int raw; };
struct Holder { union Outer o; char tail; };
int main(void) {
    struct Holder h;
    h.o.s.tag = 'x';
    h.o.raw = 3;
    return h.tail;
}
"#,
    );
    let outer = program.structs.get("Outer").expect("Outer laid out");
    assert_eq!(outer.size, 8);
    assert_eq!(outer.align, 4);
    let holder = program.structs.get("Holder").expect("Holder laid out");
    assert_eq!(holder.fields[0].offset, 0);
    assert_eq!(holder.fields[1].offset, 8);
    assert_eq!(holder.size, 12);
}

// ---- bit-fields -----------------------------------------------------------

fn bitfield(def: &polyglot_core::StructDef, name: &str) -> BitfieldLayout {
    def.fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("field `{name}` present"))
        .bitfield
        .unwrap_or_else(|| panic!("field `{name}` is a bit-field"))
}

#[test]
fn unnamed_bitfields_do_not_affect_alignment() {
    let program = checked(
        r#"
struct T { char c; unsigned :5; char d; };
struct V { char c; unsigned p:5; char d; };
"#,
    );
    let t = program.structs.get("T").unwrap();
    assert_eq!(t.size, 3, "unnamed padding adds no alignment");
    assert_eq!(t.align, 1);
    assert_eq!(t.fields.iter().find(|f| f.name == "d").unwrap().offset, 2);
    let v = program.structs.get("V").unwrap();
    assert_eq!(v.size, 4, "the named variant aligns to its unit");
    assert_eq!(v.align, 4);
}

#[test]
fn bitfields_pack_within_storage_units() {
    let program = checked(
        r#"
struct B { unsigned a:3; unsigned b:5; unsigned c:7; };
struct C { unsigned a:30; unsigned b:5; };
struct K { unsigned a:12; unsigned b:12; unsigned c:12; };
struct N { unsigned a:4; unsigned b:28; };
"#,
    );
    let b = program.structs.get("B").unwrap();
    assert_eq!(b.size, 4);
    assert_eq!(bitfield(b, "a").lsb, 0);
    assert_eq!(bitfield(b, "b").lsb, 3);
    assert_eq!(bitfield(b, "c").lsb, 8);
    let c = program.structs.get("C").unwrap();
    assert_eq!(c.size, 8, "b:5 cannot fit after a:30, so a new unit starts");
    assert_eq!(c.fields.iter().find(|f| f.name == "b").unwrap().offset, 4);
    assert_eq!(bitfield(c, "b").lsb, 0);
    let k = program.structs.get("K").unwrap();
    assert_eq!(k.size, 8);
    assert_eq!(bitfield(k, "a").lsb, 0);
    assert_eq!(bitfield(k, "b").lsb, 12);
    assert_eq!(bitfield(k, "c").lsb, 0);
    assert_eq!(
        k.fields.iter().find(|f| f.name == "c").unwrap().offset,
        4,
        "c:12 starts a second storage unit"
    );
    let n = program.structs.get("N").unwrap();
    assert_eq!(n.size, 4, "4 + 28 bits exactly fill one int unit");
    assert_eq!(bitfield(n, "b").lsb, 4);
}

#[test]
fn bitfield_unit_types_follow_the_declared_type() {
    let program = checked(
        r#"
struct A { char c; int b:4; };
struct D { char a:4; char b:8; };
struct G { unsigned long long a:60; char b; };
struct I { char c; long b:4; };
struct R { long x:32; char c; };
"#,
    );
    // char c; int b:4 -> b shares the int unit at offset 0, lsb 8.
    let a = program.structs.get("A").unwrap();
    assert_eq!(a.size, 4);
    assert_eq!(a.align, 4);
    assert_eq!(bitfield(a, "b").lsb, 8);
    // char a:4; char b:8 -> b needs a fresh byte unit.
    let d = program.structs.get("D").unwrap();
    assert_eq!(d.size, 2);
    assert_eq!(d.align, 1);
    assert_eq!(d.fields.iter().find(|f| f.name == "b").unwrap().offset, 1);
    assert_eq!(bitfield(d, "b").lsb, 0);
    // 64-bit units.
    let g = program.structs.get("G").unwrap();
    assert_eq!(g.size, 16);
    assert_eq!(g.align, 8);
    assert_eq!(g.fields.iter().find(|f| f.name == "b").unwrap().offset, 8);
    let i = program.structs.get("I").unwrap();
    assert_eq!(i.size, 8);
    assert_eq!(i.align, 8);
    assert_eq!(bitfield(i, "b").lsb, 8);
    let r = program.structs.get("R").unwrap();
    assert_eq!(r.size, 8);
    let rbf = bitfield(r, "x");
    assert_eq!(rbf.width, 32);
    assert!(rbf.signed);
    assert_eq!(r.fields.iter().find(|f| f.name == "c").unwrap().offset, 4);
}

#[test]
fn zero_width_and_unnamed_bitfields_act_as_boundaries() {
    let program = checked(
        r#"
struct E { int a:4; int :0; int b:4; };
struct F { char c; int :0; char d; };
struct L { unsigned a:3; unsigned :5; unsigned b:8; };
"#,
    );
    let e = program.structs.get("E").unwrap();
    assert_eq!(e.size, 8, "`:0` closes the int unit before b");
    assert_eq!(e.fields[1].name, "", "`:0` is unnamed");
    assert_eq!(bitfield(e, "b").lsb, 0);
    assert_eq!(e.fields.iter().find(|f| f.name == "b").unwrap().offset, 4);
    let f = program.structs.get("F").unwrap();
    assert_eq!(f.size, 5);
    assert_eq!(f.align, 1, "unnamed bit-fields do not affect alignment");
    assert_eq!(f.fields.iter().find(|f| f.name == "d").unwrap().offset, 4);
    let l = program.structs.get("L").unwrap();
    assert_eq!(l.size, 4);
    let unnamed = &l.fields[1];
    assert_eq!(unnamed.name, "");
    assert_eq!(unnamed.bitfield.unwrap().width, 5, "unnamed padding field");
    assert_eq!(bitfield(l, "b").lsb, 8);
}

#[test]
fn union_bitfields_overlap_from_bit_zero() {
    let program = checked(
        r#"
union V { unsigned a:3; unsigned b:5; };
union W { int :0; char c; };
"#,
    );
    let v = program.structs.get("V").unwrap();
    assert_eq!(v.size, 4, "named bit-fields occupy a whole unit");
    assert_eq!(v.align, 4);
    assert_eq!(bitfield(v, "a").lsb, 0);
    assert_eq!(bitfield(v, "b").lsb, 0);
    let w = program.structs.get("W").unwrap();
    assert_eq!(w.size, 1, "unnamed bit-fields contribute nothing to unions");
}

#[test]
fn bitfield_struct_member_ops_pass_checking() {
    checked(
        r#"
struct Flags { unsigned a:4; unsigned b:4; int big:20; };
int main(void) {
    struct Flags f;
    f.a = 5;
    f.b = f.a + 2;
    f.big = -3;
    f.a++;
    f.b <<= 1;
    if (f.a) { return f.b; }
    return sizeof(f) + sizeof(struct Flags);
}
"#,
    );
}

#[test]
fn rejects_address_of_bitfield() {
    let diags = check_errors(
        r#"
struct Flags { unsigned a:4; int b; };
int main(void) {
    struct Flags f;
    int *p = &f.a;
    return *p;
}
"#,
    );
    assert!(
        has_code(&diags, "E0348"),
        "cannot take the address of a bit-field"
    );
    // Ordinary members are still fine.
    checked(
        r#"
struct Flags { unsigned a:4; int b; };
int main(void) {
    struct Flags f;
    int *p = &f.b;
    return *p;
}
"#,
    );
}

#[test]
fn unnamed_bitfields_take_no_initializer() {
    let program = checked(
        r#"
struct Padded { unsigned a:4; unsigned :4; unsigned b:8; };
int main(void) {
    struct Padded p = { 3, 9 };
    return p.b + p.a;
}
"#,
    );
    assert!(program.structs.contains_key("Padded"));
    let diags = check_errors(
        r#"
struct Padded { unsigned a:4; unsigned :4; unsigned b:8; };
int main(void) {
    struct Padded p = { 3, 9, 1 };
    return p.b;
}
"#,
    );
    assert!(has_code(&diags, "E0322"), "too many initializers");
}

#[test]
fn bitfield_widths_beyond_the_type_are_rejected() {
    let diags = check_errors(
        r#"
struct Bad { int x:33; };
int main(void) { return 0; }
"#,
    );
    // Parse-level: width exceeds the 32-bit int unit.
    assert!(has_code(&diags, "E0216") || has_code(&diags, "E0214"));
}

#[test]
fn switch_and_enums_compose() {
    // Enumerators fold to constants usable as case labels.
    checked(
        r#"
enum State { IDLE, BUSY = 5, DONE };
int step(enum State s) {
    switch (s) {
        case IDLE: return 0;
        case BUSY: return 1;
        case DONE: return 2;
    }
    return -1;
}
"#,
    );
}

#[test]
fn sizeof_folds_for_new_types() {
    let program = checked(
        r#"
union U { int a; char b[5]; };
struct A { char c; int b:4; };
int main(void) {
    int x = sizeof(union U);
    int y = sizeof(struct A);
    return x + y;
}
"#,
    );
    let body = program
        .unit
        .funcs
        .iter()
        .find(|f| f.name == "main")
        .and_then(|f| f.body.as_ref())
        .expect("main body");
    let mut sizes = Vec::new();
    for s in body {
        if let polyglot_syntax::ast::StmtKind::Decl {
            init: Some(polyglot_syntax::ast::Init::Expr(e)),
            ..
        } = &s.kind
        {
            if let polyglot_syntax::ast::ExprKind::IntLit(v, Suffix::Long) = e.kind {
                sizes.push(v);
            }
        }
    }
    assert_eq!(sizes, vec![8, 4], "sizeof folds to the laid-out sizes");
}
