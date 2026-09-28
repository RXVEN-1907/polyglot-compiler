//! Integration tests for lowering the newer language features:
//! `switch` (dispatch chain + fallthrough), `enum` constants, `union`
//! member access and struct bit-fields (shift/mask, read-modify-write).
//!
//! Lowering stays within the existing IR primitives: no new `Instr`
//! kinds or terminators are introduced for any of these features.

use polyglot_core::DiagnosticEngine;
use polyglot_core::Type;
use polyglot_ir::print::print_function;
use polyglot_ir::{lower_program, Function, Instr, Module, Terminator};
use polyglot_syntax::ast::BinOp;

fn lowered(src: &str) -> Module {
    let mut diags = DiagnosticEngine::new();
    let (_, unit) = parse_source_checked(src, &mut diags);
    let checked = polyglot_sema::check(unit, &mut diags).expect("type checking should pass");
    lower_program(checked)
}

fn parse_source_checked(
    src: &str,
    diags: &mut DiagnosticEngine,
) -> (polyglot_core::SourceFile, polyglot_syntax::TranslationUnit) {
    let parsed =
        polyglot_syntax::parse_source("test.c", src, &[], diags).expect("parse should not crash");
    assert!(
        !diags.has_errors(),
        "parse errors: {}",
        diags.diagnostics().len()
    );
    parsed
}

fn func<'m>(module: &'m Module, name: &str) -> &'m Function {
    module
        .funcs
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function `{name}` present"))
}

fn printed_func(f: &Function) -> String {
    let mut out = String::new();
    print_function(f, &mut out);
    out
}

fn const_value(instr: &Instr) -> Option<i128> {
    match instr {
        Instr::Const { value, .. } => Some(*value),
        _ => None,
    }
}

fn has_const(f: &Function, value: i128) -> bool {
    f.blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .any(|i| const_value(i) == Some(value))
}

fn first_block_with_const(f: &Function, value: i128) -> usize {
    f.blocks
        .iter()
        .position(|b| b.instrs.iter().any(|i| const_value(i) == Some(value)))
        .unwrap_or_else(|| panic!("const {value} present:\n{}", printed_func(f)))
}

// ---- switch ----------------------------------------------------------------

#[test]
fn optimized_switch_keeps_scrutinee_uses_defined() {
    // Regression: the simplifier used to delete a load-after-store and
    // rewrite its uses only within the same block, leaving the switch
    // dispatch chain (which reads the scrutinee register in later
    // blocks) with an undefined register — every case missed and the
    // default ran instead.
    let module = lowered(
        r#"
int printf(char *, ...);
int main() {
    int e = 4;
    switch (e) {
        case 3: return 1;
        case 4: return 2;
        default: return 3;
    }
}
"#,
    );
    let mut module = module;
    polyglot_ir::optimize_module(&mut module, polyglot_ir::OptLevel::Default);
    let main = func(&module, "main");
    let printed = printed_func(main);
    // Every register used anywhere must have a definition: either a
    // parameter register or some instruction's destination.
    let defined: std::collections::HashSet<u32> = main
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter_map(Function::dest)
        .chain(0..(main.param_tys.len() as u32))
        .collect();
    let used: std::collections::HashSet<u32> = main
        .blocks
        .iter()
        .flat_map(|b| {
            b.instrs
                .iter()
                .flat_map(Function::uses)
                .chain(b.term.uses())
        })
        .collect();
    let dangling: Vec<_> = used.difference(&defined).collect();
    assert!(
        dangling.is_empty(),
        "registers {dangling:?} used without a definition:\n{printed}"
    );
    // And the dispatch chain still compares against both case values.
    assert!(has_const(main, 3) && has_const(main, 4));
}

#[test]
fn lowers_stacked_case_labels_without_dangling_branches() {
    // Regression: with stacked labels (`case 1: case 2:`) the dispatch
    // chain used to branch into an empty `unreachable` block whenever
    // the first label of a stack did not match.
    let module = lowered(
        r#"
int printf(char *, ...);
int main(int v) {
    switch (v) {
        case 1:
        case 2: printf("small\n"); break;
        case 3: printf("three\n");
        default: printf("fell\n"); break;
    }
    return 0;
}
"#,
    );
    let main = func(&module, "main");
    let printed = printed_func(main);
    // Every branch target must be a live block (never one terminated
    // by `unreachable`).
    for (idx, b) in main.blocks.iter().enumerate() {
        let targets: Vec<u32> = match &b.term {
            Terminator::Br(t) => vec![*t],
            Terminator::CondBr { then, else_, .. } => vec![*then, *else_],
            _ => vec![],
        };
        for t in targets {
            assert!(
                !matches!(main.blocks[t as usize].term, Terminator::Unreachable),
                "block bb{idx} branches to dead bb{t}:\n{printed}"
            );
        }
    }
    // The stacked labels share one body: both case constants compare
    // against the same section body block.
    assert!(has_const(main, 1) && has_const(main, 2) && has_const(main, 3));
}

#[test]
fn lowers_switch_dispatch_chain() {
    let module = lowered(
        r#"
int pick(int v) {
    switch (v) {
        case 1: return 10;
        case 2: return 20;
        default: return 0;
    }
}
"#,
    );
    let pick = func(&module, "pick");
    // One conditional branch per case label, each comparing the
    // scrutinee against the case constant.
    let cond_blocks: Vec<&polyglot_ir::Block> = pick
        .blocks
        .iter()
        .filter(|b| matches!(b.term, Terminator::CondBr { .. }))
        .collect();
    assert_eq!(
        cond_blocks.len(),
        2,
        "one dispatch check per case label:\n{}",
        printed_func(pick)
    );
    for b in &cond_blocks {
        assert!(b
            .instrs
            .iter()
            .any(|i| matches!(i, Instr::Binop { op: BinOp::Eq, .. })));
    }
    // Case bodies return their constants; default returns 0.
    assert!(has_const(pick, 10) && has_const(pick, 20) && has_const(pick, 0));
    for b in &cond_blocks {
        match &b.term {
            Terminator::CondBr { then, else_, .. } => {
                // The matched branch enters the section body, which returns.
                assert!(matches!(
                    pick.blocks[*then as usize].term,
                    Terminator::Return(Some(_))
                ));
                // The unmatched branch continues the dispatch chain.
                assert!(matches!(
                    pick.blocks[*else_ as usize].term,
                    Terminator::CondBr { .. } | Terminator::Br(_)
                ));
            }
            _ => unreachable!(),
        }
    }
    // Fall out of the chain into the default body.
    let last_check = cond_blocks[1].term.clone();
    match last_check {
        Terminator::CondBr { else_, .. } => match &pick.blocks[else_ as usize].term {
            Terminator::Br(t) => {
                assert!(matches!(
                    pick.blocks[*t as usize].term,
                    Terminator::Return(Some(_))
                ));
            }
            other => panic!("fallback jumps to the default body, got {other:?}"),
        },
        other => panic!("chain continues, got {other:?}"),
    }
}

#[test]
fn lowers_switch_fallthrough_between_sections() {
    let module = lowered(
        r#"
int step(int v) {
    int x = 0;
    switch (v) {
        case 1: x = 1;
        case 2: x = 2; break;
        case 3: x = 3;
    }
    return x;
}
"#,
    );
    let step = func(&module, "step");
    // Section one stores 1 and falls through into the block that
    // stores 2 (no implicit break).
    let b1 = first_block_with_const(step, 1);
    let b2 = first_block_with_const(step, 2);
    match &step.blocks[b1].term {
        Terminator::Br(t) => {
            assert_eq!(
                *t as usize,
                b2,
                "case 1 falls through into case 2's body:\n{}",
                printed_func(step)
            );
        }
        other => panic!("fallthrough lowers to a direct branch, got {other:?}"),
    }
    // `break` inside the second section jumps past the switch.
    match &step.blocks[b2].term {
        Terminator::Br(t) => {
            // The end block is created before the section bodies and
            // leads to the merge/return; it must not be another body.
            let target = *t as usize;
            assert_ne!(target, b1, "break escapes the switch");
            assert!(!step.blocks[target]
                .instrs
                .iter()
                .any(|i| const_value(i) == Some(3)));
        }
        other => panic!("break lowers to a branch, got {other:?}"),
    }
    // The unreachable case-3 body still exists after the break path.
    assert!(has_const(step, 3));
}

#[test]
fn lowers_switch_without_default_falls_out() {
    let module = lowered(
        r#"
int f(int v) {
    switch (v) {
        case 'a': return 97;
        case -5: return 1;
    }
    return 0;
}
"#,
    );
    let f = func(&module, "f");
    // Char constants and negative constants are plain integer cases.
    assert!(has_const(f, 97));
    assert!(has_const(f, -5));
    // No default: falling out of the dispatch chain reaches the code
    // after the switch (the trailing `return 0`).
    let cond_blocks: Vec<usize> = f
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| matches!(b.term, Terminator::CondBr { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(cond_blocks.len(), 2);
    for &i in &cond_blocks {
        match &f.blocks[i].term {
            Terminator::CondBr { then, else_, .. } => {
                // Bodies return; the chain continues towards the exit.
                let then_block = &f.blocks[*then as usize];
                assert!(matches!(then_block.term, Terminator::Return(Some(_))));
                let else_block = &f.blocks[*else_ as usize];
                assert!(matches!(
                    else_block.term,
                    Terminator::CondBr { .. } | Terminator::Br(_)
                ));
            }
            _ => unreachable!(),
        }
    }
}

// ---- enums -----------------------------------------------------------------

#[test]
fn lowers_enum_constants_to_integer_consts() {
    let module = lowered(
        r#"
enum Color { RED, GREEN, BLUE = 10, NEXT };
int mix(void) {
    return RED + NEXT + BLUE;
}
"#,
    );
    let mix = func(&module, "mix");
    assert!(
        has_const(mix, 0) && has_const(mix, 11) && has_const(mix, 10),
        "enumerators lower to their integer values:\n{}",
        printed_func(mix)
    );
    // They participate in ordinary arithmetic.
    assert!(mix
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .any(|i| matches!(i, Instr::Binop { op: BinOp::Add, .. })));
}

#[test]
fn lowers_enum_driven_switch() {
    let module = lowered(
        r#"
enum State { IDLE, BUSY };
int step(enum State s) {
    switch (s) {
        case IDLE: return 1;
        case BUSY: return 2;
    }
    return 0;
}
"#,
    );
    let step = func(&module, "step");
    assert!(has_const(step, 0) && has_const(step, 1) && has_const(step, 2));
    assert_eq!(
        step.blocks
            .iter()
            .filter(|b| matches!(b.term, Terminator::CondBr { .. }))
            .count(),
        2
    );
}

// ---- unions ----------------------------------------------------------------

#[test]
fn lowers_union_members_at_a_shared_offset() {
    let module = lowered(
        r#"
union Value { int i; char b; };
int pun(union Value *v) {
    v->i = 300;
    return v->b;
}
"#,
    );
    let pun = func(&module, "pun");
    // The int store and the char re-read both target offset 0: the
    // members genuinely overlap.
    let mut int_store_at_0 = false;
    let mut char_load_at_0 = false;
    for b in &pun.blocks {
        for i in &b.instrs {
            match i {
                Instr::Store { offset: 0, ty, .. } if *ty == Type::int() => int_store_at_0 = true,
                Instr::Load { offset: 0, ty, .. } if *ty == Type::char_t() => char_load_at_0 = true,
                _ => {}
            }
        }
    }
    let printed = printed_func(pun);
    assert!(
        int_store_at_0 && char_load_at_0,
        "union members overlap at offset 0:\n{printed}"
    );
}

#[test]
fn lowers_union_assignment_as_memcpy() {
    let module = lowered(
        r#"
union Value { int i; char bytes[8]; };
void copy(union Value *dst, union Value *src) {
    *dst = *src;
}
"#,
    );
    let copy = func(&module, "copy");
    assert!(
        copy.blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .any(|i| matches!(i, Instr::Memcpy { size: 8, .. })),
        "whole-union assignment copies the full union size:\n{}",
        printed_func(copy)
    );
    // A union local with a braced initializer zero-fills the whole
    // union before the members are stored.
    let module2 = lowered(
        r#"
union Value { int i; char bytes[8]; };
int make(void) {
    union Value v = { 5 };
    return v.i;
}
"#,
    );
    let zero = func(&module2, "make");
    assert!(
        has_const(zero, 8),
        "memset of the union local covers all 8 bytes:\n{}",
        printed_func(zero)
    );
    assert!(has_const(zero, 5), "the first member is initialized");
}

// ---- bit-fields ------------------------------------------------------------

#[test]
fn lowers_bitfield_read_with_shift_and_mask() {
    let module = lowered(
        r#"
struct S { unsigned a:4; unsigned b:4; };
int read_b(struct S *s) {
    return s->b;
}
"#,
    );
    let f = func(&module, "read_b");
    // Load the storage unit, shift the field down, mask to its width.
    let printed = printed_func(f);
    assert!(
        f.blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .any(|i| matches!(i, Instr::Load { offset: 0, ty, .. } if *ty == Type::int())),
        "the int storage unit is loaded:\n{printed}"
    );
    assert!(
        has_const(f, 4) && has_const(f, 15),
        "shift by lsb 4 and mask 0xF:\n{printed}"
    );
    assert!(f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .any(|i| matches!(i, Instr::Binop { op: BinOp::Shr, .. })));
    assert!(f.blocks.iter().flat_map(|b| &b.instrs).any(|i| matches!(
        i,
        Instr::Binop {
            op: BinOp::BitAnd,
            ..
        }
    )));
}

#[test]
fn lowers_bitfield_write_as_read_modify_write() {
    let module = lowered(
        r#"
struct S { unsigned a:4; unsigned b:4; };
int set_b(struct S *s) {
    s->b = 6;
    return s->b;
}
"#,
    );
    let f = func(&module, "set_b");
    // The stored unit keeps neighbouring bits: the old unit is loaded,
    // cleared with ~ (0xF << 4), merged with (6 & 0xF) << 4.
    let unit_mask = (1i128 << 32) - 1;
    let clear_mask = !((0xFi128) << 4) & unit_mask; // 0xFFFFFF0F
    assert!(
        has_const(f, clear_mask),
        "clear mask keeps neighbouring fields:\n{}",
        printed_func(f)
    );
    assert!(has_const(f, 15), "value truncated to the field width");
    let instrs: Vec<&Instr> = f.blocks.iter().flat_map(|b| &b.instrs).collect();
    assert!(instrs
        .iter()
        .any(|i| matches!(i, Instr::Binop { op: BinOp::Shl, .. })));
    assert!(instrs.iter().any(|i| matches!(
        i,
        Instr::Binop {
            op: BinOp::BitOr,
            ..
        }
    )));
    assert!(instrs
        .iter()
        .any(|i| matches!(i, Instr::Store { offset: 0, .. })));
}

#[test]
fn lowers_signed_bitfield_read_with_sign_extension() {
    let module = lowered(
        r#"
struct S { int a:4; };
int read_a(struct S *s) {
    return s->a;
}
"#,
    );
    let f = func(&module, "read_a");
    // Sign extension: value -= ((value >> (w-1)) & 1) << w.
    let printed = printed_func(f);
    assert!(has_const(f, 3), "w-1 shift:\n{printed}");
    assert!(has_const(f, 4), "width shift:\n{printed}");
    let has_sub_then = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .any(|i| matches!(i, Instr::Binop { op: BinOp::Sub, .. }));
    assert!(has_sub_then, "bias subtraction present:\n{printed}");
}

#[test]
fn lowers_bitfield_incdec_via_rmw() {
    let module = lowered(
        r#"
struct S { unsigned a:4; };
void bump(struct S *s) {
    s->a += 1;
    s->a++;
}
"#,
    );
    let f = func(&module, "bump");
    let printed = printed_func(f);
    // Both updates go through load-modify-store of the unit.
    let stores = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| matches!(i, Instr::Store { offset: 0, .. }))
        .count();
    assert!(
        stores >= 2,
        "each bit-field update stores the unit:\n{printed}"
    );
    assert!(f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .any(|i| matches!(i, Instr::Binop { op: BinOp::Add, .. })));
    assert!(has_const(f, 15), "field width mask in play");
}

#[test]
fn lowers_bitfield_struct_initializer_without_clobbering() {
    let module = lowered(
        r#"
struct S { unsigned a:4; unsigned b:4; };
int make(void) {
    struct S s = { 3, 9 };
    return s.b;
}
"#,
    );
    let f = func(&module, "make");
    let printed = printed_func(f);
    // Both members share the unit at offset 0; initializers must merge
    // (read-modify-write) rather than overwrite the whole unit.
    let rmw_stores = f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter(|i| matches!(i, Instr::Store { offset: 0, .. }))
        .count();
    assert_eq!(
        rmw_stores, 2,
        "each bit-field initializer merges into the unit:\n{printed}"
    );
    // Reading b back shifts by its lsb.
    assert!(has_const(f, 4) && has_const(f, 15));
}

#[test]
fn zero_width_bitfield_shifts_following_members() {
    let module = lowered(
        r#"
struct S { unsigned a:4; int :0; unsigned b:4; };
int read_b(struct S *s) {
    return s->b;
}
"#,
    );
    let f = func(&module, "read_b");
    let printed = printed_func(f);
    // b lives in the second int unit (offset 4 baked into the member
    // address), at lsb 0: only a mask is needed, no shift.
    assert!(
        has_const(f, 4),
        "the member address carries the second unit's offset:\n{printed}"
    );
    assert!(has_const(f, 15), "masked to 4 bits:\n{printed}");
    assert!(
        !f.blocks
            .iter()
            .flat_map(|b| &b.instrs)
            .any(|i| matches!(i, Instr::Binop { op: BinOp::Shr, .. })),
        "lsb 0 needs no shift:\n{printed}"
    );
    assert!(f
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .any(|i| matches!(i, Instr::Load { ty, .. } if *ty == Type::int())));
}
