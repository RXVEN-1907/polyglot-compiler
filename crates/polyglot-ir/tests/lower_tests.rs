//! Integration tests for AST -> IR lowering (`polyglot-ir::lower`).
//!
//! Each test lowers a small program through parse -> check -> lower and
//! asserts structural properties of the resulting module: function and
//! block counts, terminators, slot tables and instruction kinds.

use polyglot_core::DiagnosticEngine;
use polyglot_ir::print::print_function;
use polyglot_ir::{lower_program, Function, Instr, Module, Terminator};
use polyglot_syntax::parse_source;

fn lowered(src: &str) -> Module {
    let mut diags = DiagnosticEngine::new();
    let (_, unit) = parse_source("test.c", src, &[], &mut diags).expect("parse should not crash");
    assert!(
        !diags.has_errors(),
        "parse errors: {}",
        diags.diagnostics().len()
    );
    let checked = polyglot_sema::check(unit, &mut diags).expect("type checking should pass");
    lower_program(checked)
}

fn func<'m>(module: &'m Module, name: &str) -> &'m Function {
    module
        .funcs
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function `{name}` present"))
}

fn block_of(f: &Function, id: u32) -> &polyglot_ir::Block {
    &f.blocks[id as usize]
}

fn printed_func(f: &Function) -> String {
    let mut out = String::new();
    print_function(f, &mut out);
    out
}

/// Block ids reachable from the entry block by following terminators.
fn reachable_blocks(f: &Function) -> std::collections::HashSet<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut work = vec![0u32];
    while let Some(b) = work.pop() {
        if !seen.insert(b) {
            continue;
        }
        match &f.blocks[b as usize].term {
            Terminator::Br(t) => work.push(*t),
            Terminator::CondBr { then, else_, .. } => {
                work.push(*then);
                work.push(*else_);
            }
            _ => {}
        }
    }
    seen
}

// ---- globals ---------------------------------------------------------------

#[test]
fn lowers_globals_with_initializers() {
    let module = lowered(
        r#"
int counter = 7;
char msg[6];
int zeroed;
int main() {
    counter = counter + 1;
    msg[0] = 'h';
    return counter;
}
"#,
    );
    let by_name = |name: &str| module.globals.iter().find(|g| g.name == name).unwrap();
    // `counter` keeps its 4-byte little-endian initializer.
    let counter = by_name("counter");
    assert_eq!(counter.size, 4);
    match &counter.init {
        polyglot_ir::GlobalInit::Bytes(b) => assert_eq!(b, &vec![7u8, 0, 0, 0]),
        other => panic!("counter should be byte-initialized, got {other:?}"),
    }
    // Uninitialized globals are zero-filled with their declared size.
    let msg = by_name("msg");
    assert_eq!(msg.size, 6);
    assert!(matches!(msg.init, polyglot_ir::GlobalInit::Zero));
    let zeroed = by_name("zeroed");
    assert_eq!(zeroed.size, 4);
    assert!(matches!(zeroed.init, polyglot_ir::GlobalInit::Zero));

    // main touches globals through load/store instructions.
    let main = func(&module, "main");
    let mut load_counter = false;
    let mut store_counter = false;
    let mut typed_char_store = false;
    for b in &main.blocks {
        for i in &b.instrs {
            match i {
                Instr::LoadGlobal { name, .. } if name == "counter" => load_counter = true,
                Instr::StoreGlobal { name, .. } if name == "counter" => store_counter = true,
                Instr::Store { ty, .. } if *ty == polyglot_core::Type::char_t() => {
                    typed_char_store = true
                }
                _ => {}
            }
        }
    }
    let printed = printed_func(main);
    assert!(
        load_counter,
        "reads of globals lower to load.global:\n{printed}"
    );
    assert!(
        store_counter,
        "global assignment lowers to store.global:\n{printed}"
    );
    assert!(
        typed_char_store,
        "msg[0] = 'h' lowers to a typed char store:\n{printed}"
    );
}

// ---- calls -----------------------------------------------------------------

#[test]
fn lowers_function_calls_and_params() {
    let module = lowered(
        r#"
int add(int a, int b) { return a + b; }
int twice(int v) { return add(v, v); }
int main() { return twice(21); }
"#,
    );
    // Exactly the three source functions (memset only joins the
    // signature table, not the module).
    assert_eq!(module.funcs.len(), 3);
    assert!(module.funcs.iter().all(|f| f.is_definition));

    let add = func(&module, "add");
    assert_eq!(
        add.param_tys,
        vec![polyglot_core::Type::int(), polyglot_core::Type::int()]
    );
    // A straight-line body: exactly one reachable block, ending in the
    // explicit return.
    let reachable = reachable_blocks(add);
    assert_eq!(
        reachable.len(),
        1,
        "a straight-line body lowers to one reachable block:\n{}",
        printed_func(add)
    );
    assert!(reachable.contains(&0));
    assert!(matches!(add.blocks[0].term, Terminator::Return(Some(_))));
    assert!(
        add.blocks[0].instrs.iter().any(|i| matches!(
            i,
            Instr::Binop {
                op: polyglot_syntax::ast::BinOp::Add,
                ..
            }
        )),
        "the body computes an add"
    );
    // Parameters become named slots initialized at entry.
    assert_eq!(add.slots.len(), 2);
    assert_eq!(add.slots[0].name, "a");
    assert_eq!(add.slots[1].name, "b");

    let twice = func(&module, "twice");
    let call = twice.blocks[0]
        .instrs
        .iter()
        .find(|i| matches!(i, Instr::Call { .. }))
        .expect("twice calls add");
    match call {
        Instr::Call { dest, func, args } => {
            assert_eq!(func, "add");
            assert_eq!(args.len(), 2, "both arguments are passed");
            assert!(dest.is_some(), "the non-void result has a register");
        }
        _ => unreachable!(),
    }

    let main = func(&module, "main");
    assert!(main.blocks.iter().any(|b| b
        .instrs
        .iter()
        .any(|i| matches!(i, Instr::Call { func, .. } if func == "twice"))));
}

// ---- control flow ----------------------------------------------------------

#[test]
fn lowers_if_else_diamond() {
    let module = lowered(
        r#"
int pick(int v) {
    int r;
    if (v > 0) { r = 1; } else { r = 2; }
    return r;
}
"#,
    );
    let pick = func(&module, "pick");
    // The diamond shape: entry, then, else and merge blocks come first;
    // the lowerer may leave additional unreachable blocks after them.
    assert!(
        pick.blocks.len() >= 4,
        "diamond needs four live blocks:\n{}",
        printed_func(pick)
    );
    match &pick.blocks[0].term {
        Terminator::CondBr { then, else_, .. } => {
            assert_eq!(*then, 1, "then-block follows the entry");
            assert_eq!(*else_, 2, "else-block follows the then-block");
        }
        other => panic!("entry ends in a conditional branch, got {other:?}"),
    }
    // Both arms branch to the shared merge block, which returns.
    let branches_to_merge = |id: u32| matches!(&pick.blocks[id as usize].term, Terminator::Br(3));
    assert!(branches_to_merge(1) && branches_to_merge(2));
    assert!(matches!(pick.blocks[3].term, Terminator::Return(Some(_))));
    assert!(block_of(pick, 3)
        .instrs
        .iter()
        .any(|i| matches!(i, Instr::LoadLocal { .. })));
}

#[test]
fn lowers_while_loop_with_back_edge() {
    let module = lowered(
        r#"
int countdown(int n) {
    while (n > 0) { n = n - 1; }
    return n;
}
"#,
    );
    let cd = func(&module, "countdown");
    assert!(
        cd.blocks.len() >= 3,
        "entry, loop head and body blocks:\n{}",
        printed_func(cd)
    );
    // The loop head tests the condition.
    assert!(matches!(cd.blocks[1].term, Terminator::CondBr { .. }));
    // Some branch jumps backwards to the loop head: the back edge.
    let has_back_edge = cd
        .blocks
        .iter()
        .enumerate()
        .any(|(idx, b)| matches!(&b.term, Terminator::Br(target) if (*target as usize) < idx));
    assert!(has_back_edge, "while lowers a backward branch to the head");
    // The body decrements n through its slot.
    let body_decrements = cd.blocks[2..].iter().flat_map(|b| &b.instrs).any(|i| {
        matches!(
            i,
            Instr::Binop {
                op: polyglot_syntax::ast::BinOp::Sub,
                ..
            }
        )
    });
    assert!(body_decrements, "the loop body computes n - 1");
    assert_eq!(cd.slots.len(), 1);
    assert_eq!(cd.slots[0].name, "n");
}

// ---- unary ops / addressing ------------------------------------------------

#[test]
fn lowers_unary_ops_and_address_of() {
    let module = lowered(
        r#"
int ops(int a) {
    int n = -a;
    int b = ~a;
    int c = !n;
    int *p = &a;
    int d = *p;
    return n + b + c + d;
}
"#,
    );
    let ops = func(&module, "ops");
    let (mut neg, mut bnot, mut lnot) = (false, false, false);
    for b in &ops.blocks {
        for i in &b.instrs {
            match i {
                Instr::Unop {
                    op: polyglot_ir::UnOp::Neg,
                    ..
                } => neg = true,
                Instr::Unop {
                    op: polyglot_ir::UnOp::BitNot,
                    ..
                } => bnot = true,
                Instr::Unop {
                    op: polyglot_ir::UnOp::Not,
                    ..
                } => lnot = true,
                _ => {}
            }
        }
    }
    assert!(neg, "-a lowers to neg");
    assert!(bnot, "~a lowers to bnot");
    assert!(lnot, "!n lowers to lnot");
    // `&a` marks the slot so codegen keeps it in memory, and `*p` loads.
    let a_slot = ops
        .slots
        .iter()
        .position(|s| s.name == "a")
        .expect("param slot a");
    assert!(
        ops.slots[a_slot].addrof,
        "taking the address forces memory residency"
    );
    let printed = printed_func(ops);
    assert!(
        printed.contains("= addr.local"),
        "&a lowers to addr.local:\n{printed}"
    );
    assert!(
        printed.contains("= load."),
        "*p lowers to a typed load:\n{printed}"
    );
}

// ---- assignments -----------------------------------------------------------

#[test]
fn lowers_local_and_compound_assignments() {
    let module = lowered(
        r#"
int total(int n) {
    int s = 0;
    for (int i = 0; i < n; i++) { s += i; }
    s *= 2;
    return s;
}
"#,
    );
    let total = func(&module, "total");
    let mut stores = 0;
    let mut loads = 0;
    for b in &total.blocks {
        for i in &b.instrs {
            match i {
                Instr::StoreLocal { .. } => stores += 1,
                Instr::LoadLocal { .. } => loads += 1,
                _ => {}
            }
        }
    }
    assert!(
        stores >= 4,
        "init, loop step, compound add and *= all store: {stores}"
    );
    assert!(loads >= 4, "each read of s or i loads: {loads}");
    let printed = printed_func(total);
    assert!(
        printed.contains("add."),
        "s += i lowers to an add:\n{printed}"
    );
    assert!(
        printed.contains("mul."),
        "s *= 2 lowers to a mul:\n{printed}"
    );
    // Loop bookkeeping: the for-loop creates a condition branch and a
    // back edge like while.
    assert!(total
        .blocks
        .iter()
        .any(|b| matches!(b.term, Terminator::CondBr { .. })));
}
