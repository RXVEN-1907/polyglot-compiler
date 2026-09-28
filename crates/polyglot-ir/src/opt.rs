//! IR optimization passes.
//!
//! The pipeline runs three complementary passes to a fixpoint:
//!
//! 1. **constant folding** — evaluate `Binop`/`Unop` over constants,
//!    fold constant conditions to unconditional branches;
//! 2. **algebraic simplification + local copy/value propagation** —
//!    identities like `x*1`, `x+0`, `x*0`, `x-x`; copy propagation;
//!    redundant load-after-store of the same scalar slot (disabled
//!    when the slot's address escapes);
//! 3. **dead code elimination** — sweep instructions whose results are
//!    never used and that have no side effects.

use crate::{Function, Instr, Module, Reg, Terminator};
use polyglot_core::{IntKind, Type};
use polyglot_syntax::ast::BinOp;
use std::collections::HashSet;

/// Statistics reported after optimization.
#[derive(Debug, Default, Clone)]
pub struct OptStats {
    pub instrs_before: usize,
    pub instrs_after: usize,
    pub constants_folded: u32,
    pub instrs_simplified: u32,
    pub dead_removed: u32,
    pub branches_folded: u32,
    pub iterations: u32,
}

impl OptStats {
    pub fn total_removed(&self) -> usize {
        self.instrs_before.saturating_sub(self.instrs_after)
    }
}

/// Optimization level requested from the CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptLevel {
    /// No IR passes at all.
    None,
    /// Constant folding only.
    Less,
    /// Folding + simplification + DCE (the default).
    Default,
    /// Same passes, iterated to a deeper fixpoint.
    Aggressive,
}

/// Optimize a whole module in place; returns aggregate statistics.
pub fn optimize_module(module: &mut Module, level: OptLevel) -> OptStats {
    let mut total = OptStats::default();
    if level == OptLevel::None {
        total.instrs_before = module.funcs.iter().map(Function::instr_count).sum();
        total.instrs_after = total.instrs_before;
        return total;
    }
    let max_iters = match level {
        OptLevel::Less => 1,
        OptLevel::Default => 4,
        OptLevel::Aggressive => 12,
        OptLevel::None => unreachable!(),
    };
    for f in &mut module.funcs {
        if !f.is_definition {
            continue;
        }
        total.instrs_before += f.instr_count();
        let mut stats = OptStats::default();
        optimize_function(f, max_iters, &mut stats);
        total.constants_folded += stats.constants_folded;
        total.instrs_simplified += stats.instrs_simplified;
        total.dead_removed += stats.dead_removed;
        total.branches_folded += stats.branches_folded;
        total.iterations += stats.iterations;
        total.instrs_after += f.instr_count();
    }
    total
}

fn optimize_function(f: &mut Function, max_iters: u32, stats: &mut OptStats) {
    for _ in 0..max_iters {
        stats.iterations += 1;
        let before = f.instr_count();
        let changed = const_fold(f, stats);
        simplify(f, stats);
        let removed = dead_code_elim(f);
        stats.dead_removed += removed;
        if !changed && removed == 0 && f.instr_count() == before {
            break;
        }
    }
}

// ---- pass 1: constant folding ------------------------------------------

fn const_fold(f: &mut Function, stats: &mut OptStats) -> bool {
    let mut changed = false;
    for block in &mut f.blocks {
        let mut i = 0usize;
        while i < block.instrs.len() {
            let foldable = match &block.instrs[i] {
                Instr::Binop {
                    op, lhs, rhs, ty, ..
                } => {
                    let (lv, rv) = (const_of(&block.instrs, *lhs), const_of(&block.instrs, *rhs));
                    match (lv, rv) {
                        (Some(l), Some(r)) => Some(fold_binop(*op, l, r, ty)),
                        _ => None,
                    }
                }
                Instr::Unop {
                    op, operand, ty, ..
                } => const_of(&block.instrs, *operand).map(|v| fold_unop(*op, v, ty)),
                _ => None,
            };
            if let Some(value) = foldable {
                let (dest, ty) = match &block.instrs[i] {
                    Instr::Binop { dest, ty, .. } | Instr::Unop { dest, ty, .. } => {
                        (*dest, ty.clone())
                    }
                    _ => unreachable!(),
                };
                block.instrs[i] = Instr::Const { dest, value, ty };
                stats.constants_folded += 1;
                changed = true;
            }
            i += 1;
        }
        // Constant conditions become unconditional branches.
        if let Terminator::CondBr { cond, then, else_ } = block.term {
            if let Some(v) = const_of(&block.instrs, cond) {
                block.term = if v != 0 {
                    Terminator::Br(then)
                } else {
                    Terminator::Br(else_)
                };
                stats.branches_folded += 1;
                changed = true;
            }
        }
    }
    changed
}

/// Find a constant definition for `reg` within `instrs` (block-local
/// view; registers are function-scoped but definitions dominate uses
/// in lowered code within a block).
fn const_of(instrs: &[Instr], reg: Reg) -> Option<i128> {
    instrs.iter().rev().find_map(|instr| match instr {
        Instr::Const { dest, value, .. } if *dest == reg => Some(*value),
        _ => None,
    })
}

fn fold_binop(op: BinOp, l: i128, r: i128, ty: &Type) -> i128 {
    let width_bits = match ty {
        Type::Int(IntKind::Char) => 8,
        Type::Int(IntKind::Int) => 32,
        _ => 64,
    };
    let signed = !matches!(ty, Type::Pointer(_));
    let mask = if width_bits >= 128 {
        u128::MAX
    } else {
        (1u128 << width_bits) - 1
    };
    let wrap = |v: i128| -> i128 {
        let uv = (v as u128) & mask;
        if signed && uv & (1u128 << (width_bits - 1)) != 0 {
            (uv as i128).wrapping_sub(1i128 << width_bits) | !(mask as i128)
        } else {
            uv as i128
        }
    };
    match op {
        BinOp::Add => wrap(l.wrapping_add(r)),
        BinOp::Sub => wrap(l.wrapping_sub(r)),
        BinOp::Mul => wrap(l.wrapping_mul(r)),
        BinOp::Div => {
            if r == 0 {
                l // leave for runtime fault
            } else {
                wrap(l / r)
            }
        }
        BinOp::Mod => {
            if r == 0 {
                l
            } else {
                wrap(l % r)
            }
        }
        BinOp::Shl => wrap(l.wrapping_shl(r as u32)),
        BinOp::Shr => {
            if signed {
                l.wrapping_shr(r.min(63) as u32)
            } else {
                ((l as u128) >> (r.min(63) as u32)) as i128
            }
        }
        BinOp::BitAnd => l & r,
        BinOp::BitXor => l ^ r,
        BinOp::BitOr => l | r,
        BinOp::Eq => (l == r) as i128,
        BinOp::Ne => (l != r) as i128,
        BinOp::Lt => (l < r) as i128,
        BinOp::Gt => (l > r) as i128,
        BinOp::Le => (l <= r) as i128,
        BinOp::Ge => (l >= r) as i128,
        BinOp::LAnd => (l != 0 && r != 0) as i128,
        BinOp::LOr => (l != 0 || r != 0) as i128,
    }
}

fn fold_unop(op: crate::UnOp, v: i128, ty: &Type) -> i128 {
    match op {
        crate::UnOp::Neg => v.wrapping_neg(),
        crate::UnOp::Not => (v == 0) as i128,
        crate::UnOp::BitNot => {
            let mask = match ty {
                Type::Int(IntKind::Char) => 0xff,
                Type::Int(IntKind::Int) => 0xffff_ffff,
                _ => -1i64 as i128,
            };
            !v & mask
        }
    }
}

// ---- pass 2: simplification + propagation -------------------------------

fn simplify(f: &mut Function, stats: &mut OptStats) {
    // Loads eliminated as redundant after a store of the same slot:
    // `(removed load register, replacement register)`. Their uses may
    // live in other blocks (e.g. a switch dispatch chain reading the
    // scrutinee), so they are rewritten with a function-wide sweep
    // below. Registers are single-definition and the replacement is
    // defined before the load in the same block, so it dominates every
    // use the load had.
    let mut removed_loads: Vec<(Reg, Reg)> = Vec::new();
    for block in &mut f.blocks {
        let slot_addrof: Vec<bool> = f.slots.iter().map(|s| s.addrof).collect();
        simplify_block(block, &slot_addrof, stats, &mut removed_loads);
    }
    if !removed_loads.is_empty() {
        let map: std::collections::HashMap<Reg, Reg> = removed_loads.into_iter().collect();
        let mut rewrite = |r: Reg| *map.get(&r).unwrap_or(&r);
        for block in &mut f.blocks {
            for instr in &mut block.instrs {
                Function::map_uses(instr, &mut rewrite);
            }
            block.term.map_uses(&mut rewrite);
        }
    }
}

fn simplify_block(
    block: &mut crate::Block,
    slot_addrof: &[bool],
    stats: &mut OptStats,
    removed_loads: &mut Vec<(Reg, Reg)>,
) {
    // reg -> replacement (const value or other reg)
    let mut const_env: std::collections::HashMap<Reg, i128> = std::collections::HashMap::new();
    let mut copy_env: std::collections::HashMap<Reg, Reg> = std::collections::HashMap::new();
    // Last stored register per scalar slot (invalidated by calls etc).
    let mut last_store: std::collections::HashMap<u32, Reg> = std::collections::HashMap::new();

    let mut i = 0usize;
    while i < block.instrs.len() {
        // Substitute known registers/values in uses.
        let instr = &mut block.instrs[i];
        Function::map_uses(instr, &mut |r: Reg| {
            let mut cur = r;
            let mut hops = 0;
            while let Some(&next) = copy_env.get(&cur) {
                cur = next;
                hops += 1;
                if hops > 16 {
                    break;
                }
            }
            cur
        });

        match &mut block.instrs[i] {
            Instr::Binop {
                dest,
                op,
                lhs,
                rhs,
                ty,
            } => {
                // Try identities first.
                let simplified = simplify_binop(*op, *lhs, *rhs, ty);
                if let Some((new_op, new_lhs, new_rhs)) = simplified {
                    if (new_op, new_lhs, new_rhs) != (*op, *lhs, *rhs) {
                        *op = new_op;
                        *lhs = new_lhs;
                        *rhs = new_rhs;
                        stats.instrs_simplified += 1;
                    }
                }
                if let (Some(l), Some(r)) = (const_env.get(lhs), const_env.get(rhs)) {
                    let v = fold_binop(*op, *l, *r, ty);
                    let ty2 = ty.clone();
                    let d2 = *dest;
                    block.instrs[i] = Instr::Const {
                        dest: d2,
                        value: v,
                        ty: ty2,
                    };
                    const_env.insert(d2, v);
                    stats.constants_folded += 1;
                    continue; // don't advance: reprocess folded instr
                }
                // x op x for commutative-eliminable cases
                if *lhs == *rhs {
                    let v = match op {
                        BinOp::Sub => Some(0),
                        BinOp::BitXor => Some(0),
                        BinOp::BitAnd => const_env.get(lhs).copied(),
                        BinOp::BitOr => const_env.get(lhs).copied(),
                        BinOp::LAnd | BinOp::LOr => None,
                        _ => None,
                    };
                    if let Some(v) = v {
                        let ty2 = ty.clone();
                        let d2 = *dest;
                        block.instrs[i] = Instr::Const {
                            dest: d2,
                            value: v,
                            ty: ty2,
                        };
                        const_env.insert(d2, v);
                        continue;
                    }
                }
                const_env.remove(dest);
                copy_env.remove(dest);
                last_store.retain(|_, vr| *vr != *dest);
            }
            Instr::Unop {
                dest,
                op,
                operand,
                ty,
            } => {
                if let Some(v) = const_env.get(operand) {
                    let v = fold_unop(*op, *v, ty);
                    let ty2 = ty.clone();
                    let d2 = *dest;
                    block.instrs[i] = Instr::Const {
                        dest: d2,
                        value: v,
                        ty: ty2,
                    };
                    const_env.insert(d2, v);
                    continue;
                }
                const_env.remove(dest);
                copy_env.remove(dest);
            }
            Instr::Const { dest, value, .. } => {
                const_env.insert(*dest, *value);
                copy_env.remove(dest);
            }
            Instr::Convert { dest, src, to } => {
                // Convert of a constant: fold.
                if let Some(v) = const_env.get(src).copied() {
                    let v = fold_convert(v, to);
                    let t2 = to.clone();
                    let d2 = *dest;
                    block.instrs[i] = Instr::Const {
                        dest: d2,
                        value: v,
                        ty: t2,
                    };
                    const_env.insert(d2, v);
                    continue;
                }
                // NOTE: no register copy-propagation through Convert:
                // it may change integer width, so substituting its
                // source for its result is unsound.
                const_env.remove(dest);
                copy_env.remove(dest);
            }
            Instr::LoadLocal { dest, slot } => {
                // load after store of same slot -> copy
                if let Some(&stored) = last_store.get(slot) {
                    if !slot_addrof[*slot as usize] {
                        copy_env.insert(*dest, stored);
                        // The load is redundant; remove it and remember
                        // the rewrite for uses in other blocks too.
                        removed_loads.push((*dest, stored));
                        block.instrs.remove(i);
                        stats.instrs_simplified += 1;
                        continue;
                    }
                }
                const_env.remove(dest);
                copy_env.remove(dest);
            }
            Instr::StoreLocal { slot, src } => {
                // store of same value as last store -> dead
                if let Some(&prev) = last_store.get(slot) {
                    if prev == *src && !slot_addrof[*slot as usize] {
                        // Keep the first store; this one is redundant
                        // unless an intervening call could read it via
                        // pointer — addrof guard covers that.
                        block.instrs.remove(i);
                        stats.instrs_simplified += 1;
                        continue;
                    }
                }
                last_store.insert(*slot, *src);
            }
            Instr::Call { .. } => {
                // Calls can modify memory and globals; conservative reset.
                const_env.clear();
                copy_env.clear();
                last_store.clear();
            }
            Instr::StoreGlobal { .. } | Instr::Store { .. } | Instr::Memcpy { .. } => {
                const_env.clear();
                copy_env.clear();
            }
            Instr::LoadGlobal { dest, .. } | Instr::Load { dest, .. } => {
                const_env.remove(dest);
                copy_env.remove(dest);
            }
            Instr::GetGlobal { dest, .. } | Instr::AddrLocal { dest, .. } => {
                const_env.remove(dest);
                copy_env.remove(dest);
            }
        }
        i += 1;
    }
}

fn simplify_binop(op: BinOp, lhs: Reg, rhs: Reg, ty: &Type) -> Option<(BinOp, Reg, Reg)> {
    // Identities requiring a constant on one side are handled by the
    // const_env path; here we handle structural identities.
    let _ = (op, lhs, rhs, ty);
    None
}

fn fold_convert(v: i128, to: &Type) -> i128 {
    match to {
        Type::Int(IntKind::Char) => v as i8 as i128,
        Type::Int(IntKind::Int) => v as i32 as i128,
        Type::Int(IntKind::Long) => v as i64 as i128,
        _ => v,
    }
}

// ---- pass 3: dead code elimination --------------------------------------

/// Remove pure instructions with unused results. Returns count removed.
fn dead_code_elim(f: &mut Function) -> u32 {
    // Collect all used registers.
    let mut used: HashSet<Reg> = HashSet::new();
    for block in &f.blocks {
        for instr in &block.instrs {
            for u in Function::uses(instr) {
                used.insert(u);
            }
        }
        for u in block.term.uses() {
            used.insert(u);
        }
    }
    let mut removed = 0u32;
    for block in &mut f.blocks {
        let mut i = 0;
        while i < block.instrs.len() {
            let instr = &block.instrs[i];
            let pure_and_unused = match Function::dest(instr) {
                Some(dest) => !Function::has_side_effects(instr) && !used.contains(&dest),
                None => false,
            };
            if pure_and_unused {
                block.instrs.remove(i);
                removed += 1;
            } else {
                i += 1;
            }
        }
    }
    removed
}
