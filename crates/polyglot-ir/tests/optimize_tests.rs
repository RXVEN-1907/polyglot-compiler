//! Integration tests for the IR optimizer (`polyglot-ir::opt`).
//!
//! These assert the optimizer's real, documented behavior: constant
//! folding of binops/unops and constant conditions, and DCE of pure
//! unused instructions — all verified through `print_module` output
//! plus the returned [`OptStats`].

use polyglot_core::DiagnosticEngine;
use polyglot_ir::opt::{optimize_module, OptLevel, OptStats};
use polyglot_ir::print::print_module;
use polyglot_ir::{lower_program, Module};
use polyglot_syntax::parse_source;

/// Lower `src` to IR, optimize at `level`, and return the printed module.
fn optimized(src: &str, level: OptLevel) -> (String, OptStats) {
    let mut module = lowered(src);
    let stats = optimize_module(&mut module, level);
    (print_module(&module), stats)
}

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

#[test]
fn constant_arithmetic_folds_to_a_single_const() {
    let (printed, stats) = optimized(
        "int main() { int x = 2 + 3 * 4; return x; }",
        OptLevel::Default,
    );
    // 2 + 3 * 4 folds to 14; no binops survive.
    assert!(
        printed.contains("= const 14"),
        "expected folded const 14 in:\n{printed}"
    );
    assert!(
        !printed.contains(" mul."),
        "mul should be folded away:\n{printed}"
    );
    assert!(
        !printed.contains(" add."),
        "add should be folded away:\n{printed}"
    );
    assert!(
        stats.constants_folded >= 2,
        "both the mul and the add fold: {stats:?}"
    );
}

#[test]
fn unary_negation_of_constants_folds() {
    let (printed, _stats) = optimized("int main() { return -(3 * 4); }", OptLevel::Default);
    assert!(
        printed.contains("= const -12"),
        "expected const -12 in:\n{printed}"
    );
    assert!(!printed.contains(" mul."), "mul folded:\n{printed}");
    assert!(!printed.contains("= neg."), "neg folded:\n{printed}");
}

#[test]
fn dead_expression_statement_is_removed() {
    let (printed, stats) = optimized(
        "int main() { 5 + 6; int used = 7; return used; }",
        OptLevel::Default,
    );
    // The `5 + 6` statement has no observable effect: the binop and its
    // constant operands are all dead.
    assert!(!printed.contains(" add."), "dead add removed:\n{printed}");
    assert!(
        !printed.contains("= const 5"),
        "dead const removed:\n{printed}"
    );
    assert!(
        !printed.contains("= const 6"),
        "dead const removed:\n{printed}"
    );
    assert!(printed.contains("= const 7"), "live const kept:\n{printed}");
    assert!(
        stats.dead_removed >= 3,
        "two consts plus the add are dead: {stats:?}"
    );
}

#[test]
fn constant_condition_folds_to_unconditional_branch() {
    let (printed, stats) = optimized(
        "int main() { int x = 0; if (1 == 1) { x = 3; } else { x = 4; } return x; }",
        OptLevel::Default,
    );
    assert!(
        !printed.contains("br.cond"),
        "constant condition folded:\n{printed}"
    );
    assert!(
        !printed.contains("cmp.eq"),
        "comparison folded to a const:\n{printed}"
    );
    assert!(
        printed.contains("\n    br bb"),
        "unconditional branch remains:\n{printed}"
    );
    assert!(
        stats.branches_folded >= 1,
        "the constant branch is folded: {stats:?}"
    );
}

#[test]
fn none_level_leaves_module_untouched() {
    let src = "int main() { int x = 2 + 3 * 4; if (1 == 1) { return x; } return 0; }";
    let module = lowered(src);
    let before = print_module(&module);
    let mut module = module;
    let stats = optimize_module(&mut module, OptLevel::None);
    assert_eq!(stats.constants_folded, 0);
    assert_eq!(stats.branches_folded, 0);
    assert_eq!(stats.dead_removed, 0);
    assert_eq!(stats.instrs_before, stats.instrs_after);
    assert_eq!(
        print_module(&module),
        before,
        "OptLevel::None must not rewrite the IR"
    );
}

#[test]
fn opt_stats_track_pass_activity() {
    let (printed, stats) = optimized(
        "int main() { 5 + 6; int x = 2 * 3; return x; }",
        OptLevel::Default,
    );
    // Real counts: two folded binops (the dead add and 2*3), dead consts
    // removed, and the instruction count strictly shrinks.
    assert!(stats.instrs_before > stats.instrs_after, "{stats:?}");
    assert_eq!(
        stats.total_removed(),
        stats.instrs_before - stats.instrs_after
    );
    assert!(stats.constants_folded >= 2, "{stats:?}");
    assert!(stats.dead_removed >= 2, "{stats:?}");
    assert!(
        stats.iterations >= 1,
        "the pass loop ran at least once: {stats:?}"
    );
    assert!(
        !printed.contains(" add."),
        "all adds folded or dead:\n{printed}"
    );
}
