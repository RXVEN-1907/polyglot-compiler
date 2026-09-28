//! Lowering from the checked AST ([`CheckedProgram`](polyglot_sema::CheckedProgram)) to Polyglot IR.
//!
//! Blocks are tracked by index (`cur`) rather than physical position;
//! terminators immediately switch to a fresh dead block so statement
//! lowering never appends to a finished block. Trailing dead blocks
//! are pruned in `FnLowerer::finish`.

use crate::{
    ArgKind, BlockId, Function, GlobalInit, GlobalVar, Instr, Module, Reg, SlotId, Terminator,
};
use polyglot_core::{BitfieldLayout, IntKind, StructDef, Type};
use polyglot_sema::FuncSig;
use polyglot_syntax::ast::*;
use std::collections::HashMap;

/// Lower a fully checked program into IR.
pub fn lower_program(program: polyglot_sema::CheckedProgram) -> Module {
    let polyglot_sema::CheckedProgram {
        unit,
        structs,
        mut funcs,
        globals,
    } = program;

    let mut module = Module {
        structs,
        ..Module::default()
    };

    // Ensure `memset` exists so aggregate initialization can zero-fill
    // without depending on user includes.
    funcs
        .entry("memset".to_string())
        .or_insert_with(|| polyglot_sema::FuncSig {
            ret: Type::void_ptr(),
            params: vec![Type::void_ptr(), Type::int(), Type::long()],
            variadic: false,
            defined: false,
        });

    // String literal pool (interned once per unique content).
    let mut strings: Vec<String> = Vec::new();

    // Globals in source order.
    for g in &globals {
        let size = g.ty.size().max(1);
        let init = lower_global_init(&g.ty, g.init.as_ref(), &mut strings);
        module.globals.push(GlobalVar {
            name: g.name.clone(),
            ty: match &g.ty {
                Type::Array(elem, _) => (**elem).clone(),
                other => other.clone(),
            },
            size,
            init,
        });
    }

    // Functions.
    for f in &unit.funcs {
        let is_def = f.body.is_some();
        let mut function = Function {
            name: f.name.clone(),
            ret: f.ret.clone(),
            param_tys: f.params.iter().map(|p| p.ty.clone()).collect(),
            variadic: f.variadic,
            is_definition: is_def,
            slots: Vec::new(),
            blocks: Vec::new(),
            nregs: f.params.len() as Reg, // params own registers 0..n
        };
        if let Some(body) = &f.body {
            let mut lw = FnLowerer {
                cur: 0,
                post_term: false,
                func: &mut function,
                structs: &module.structs,
                funcs: &funcs,
                scopes: vec![HashMap::new()],
                breaks: Vec::new(),
                continues: Vec::new(),
                str_pool: &mut strings,
            };
            let entry = lw.new_block();
            lw.place(entry);
            // Parameters become slots initialized at entry.
            for (i, p) in f.params.iter().enumerate() {
                if p.name.is_empty() {
                    continue;
                }
                let slot = lw.func.new_slot(&p.name, p.ty.clone(), false);
                lw.emit(Instr::StoreLocal {
                    slot,
                    src: i as Reg,
                });
                lw.scopes
                    .last_mut()
                    .unwrap()
                    .insert(p.name.clone(), Binding::Slot(slot));
            }
            for stmt in body {
                // Lowering only fails on sema invariants; surface them
                // loudly rather than threading Result through the API.
                lw.lower_stmt(stmt)
                    .unwrap_or_else(|e| panic!("lowering `{}`: {e}", f.name));
            }
            lw.finish();
        }
        module.funcs.push(function);
    }

    // Append interned strings as anonymous globals.
    for s in strings.drain(..) {
        let mut bytes = s.into_bytes();
        bytes.push(0);
        let len = bytes.len() as u64;
        module.globals.push(GlobalVar {
            name: format!(".LC{}", module.globals.len()),
            ty: Type::char_t(),
            size: len,
            init: GlobalInit::Bytes(bytes),
        });
    }

    module
}

/// Byte-initialize a global object from its AST initializer.
fn lower_global_init(ty: &Type, init: Option<&Init>, strings: &mut Vec<String>) -> GlobalInit {
    let total = match ty {
        Type::Array(elem, len) => len.unwrap_or(0) * elem.size(),
        other => other.size(),
    };
    let Some(init) = init else {
        return GlobalInit::Zero;
    };
    let mut bytes = vec![0u8; total.max(1) as usize];
    match (ty, init) {
        (Type::Array(elem, _), Init::List(items)) => {
            let esz = elem.size() as usize;
            for (i, item) in items.iter().enumerate() {
                let off = i * esz;
                if off + esz > bytes.len() {
                    break;
                }
                write_const_elem(item, elem, &mut bytes[off..], strings);
            }
        }
        (Type::Array(elem, _), Init::Expr(e))
            if **elem == Type::char_t() && matches!(e.kind, ExprKind::StrLit(_)) =>
        {
            if let ExprKind::StrLit(s) = &e.kind {
                let mut b = s.clone().into_bytes();
                b.push(0);
                let n = b.len().min(bytes.len());
                bytes[..n].copy_from_slice(&b[..n]);
            }
        }
        (_, Init::Expr(_)) => write_const_elem(init, ty, &mut bytes, strings),
        _ => {}
    }
    let _ = strings;
    GlobalInit::Bytes(bytes)
}

/// Encode one constant element into `out` (little-endian, truncated).
fn write_const_elem(item: &Init, ty: &Type, out: &mut [u8], _strings: &mut Vec<String>) {
    let width = ty.size() as usize;
    if width == 0 || out.len() < width {
        return;
    }
    let value = match item {
        Init::Expr(Expr {
            kind: ExprKind::IntLit(v, _),
            ..
        }) => Some(*v),
        // Negative literals are folded by the parser, but accept them
        // here too for robustness.
        Init::Expr(Expr {
            kind: ExprKind::Unary(UnOp::Neg, inner),
            ..
        }) => match &inner.kind {
            ExprKind::IntLit(v, _) => Some(-*v),
            _ => None,
        },
        Init::Expr(Expr {
            kind: ExprKind::StrLit(s),
            ..
        }) if *ty == Type::char_t() => {
            let mut b = s.clone().into_bytes();
            b.push(0);
            let n = b.len().min(out.len());
            out[..n].copy_from_slice(&b[..n]);
            None
        }
        _ => None,
    };
    if let Some(v) = value {
        out[..width].copy_from_slice(&v.to_le_bytes()[..width]);
    }
}

#[derive(Clone, Copy, Debug)]
enum Binding {
    Slot(SlotId),
}

struct FnLowerer<'a> {
    /// Current fill target.
    cur: BlockId,
    /// True right after a terminator (current block is dead).
    post_term: bool,
    func: &'a mut Function,
    structs: &'a HashMap<String, StructDef>,
    funcs: &'a HashMap<String, FuncSig>,
    scopes: Vec<HashMap<String, Binding>>,
    breaks: Vec<BlockId>,
    continues: Vec<BlockId>,
    str_pool: &'a mut Vec<String>,
}

impl<'a> FnLowerer<'a> {
    // ---- block plumbing --------------------------------------------------

    fn new_block(&mut self) -> BlockId {
        self.func.blocks.push(crate::Block::default());
        self.func.blocks.len() as BlockId - 1
    }

    fn place(&mut self, b: BlockId) {
        while (self.func.blocks.len() as BlockId) <= b {
            self.new_block();
        }
        self.cur = b;
        self.post_term = false;
    }

    fn emit(&mut self, instr: Instr) {
        self.func.blocks[self.cur as usize].instrs.push(instr);
    }

    fn alive(&self) -> bool {
        !self.post_term
    }

    /// Terminate the current block and move to a fresh dead one.
    fn terminate(&mut self, term: Terminator) {
        self.func.blocks[self.cur as usize].term = term;
        let fresh = self.new_block();
        self.place(fresh);
    }

    /// Branch unless already dead. Returns whether the jump was needed.
    fn jump_to(&mut self, target: BlockId) -> bool {
        if self.alive() {
            self.terminate(Terminator::Br(target));
            true
        } else {
            false
        }
    }

    fn cond_branch(&mut self, cond: Reg, then_b: BlockId, else_b: BlockId) {
        self.terminate(Terminator::CondBr {
            cond,
            then: then_b,
            else_: else_b,
        });
    }

    fn const_reg(&mut self, v: i128, ty: Type) -> Reg {
        let dest = self.func.new_reg();
        self.emit(Instr::Const { dest, value: v, ty });
        dest
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn intern_string(&mut self, s: &str) -> String {
        // Deduplicate within this translation unit.
        if let Some(i) = self.str_pool.iter().position(|x| x == s) {
            format!(".LC{}", i)
        } else {
            self.str_pool.push(s.to_string());
            format!(".LC{}", self.str_pool.len() - 1)
        }
    }

    fn finish(mut self) {
        // Implicit `return 0;` / bare return at the end of the body.
        if self.alive() {
            let term = if self.func.ret.is_void() {
                Terminator::Return(None)
            } else {
                let z = self.const_reg(0, self.func.ret.clone());
                Terminator::Return(Some(z))
            };
            self.func.blocks[self.cur as usize].term = term;
        }
        // Prune trailing dead blocks.
        while let Some(last) = self.func.blocks.last() {
            if last.instrs.is_empty() && matches!(last.term, Terminator::Unreachable) {
                self.func.blocks.pop();
            } else {
                break;
            }
        }
    }

    // ---- addresses -------------------------------------------------------

    /// Materialize the address of an lvalue expression and its type.
    fn lower_addr(&mut self, expr: &Expr) -> anyhow::Result<(Reg, Type)> {
        match &expr.kind {
            ExprKind::Var(name) => {
                if let Some(Binding::Slot(slot)) = self.lookup(name) {
                    self.func.slots[slot as usize].addrof = true;
                    let dest = self.func.new_reg();
                    self.emit(Instr::AddrLocal { dest, slot });
                    Ok((dest, self.func.slots[slot as usize].ty.clone()))
                } else {
                    let dest = self.func.new_reg();
                    self.emit(Instr::GetGlobal {
                        dest,
                        name: name.clone(),
                    });
                    Ok((dest, expr.ty.clone()))
                }
            }
            ExprKind::Unary(UnOp::Deref, inner) => {
                let r = self.lower_expr(inner)?;
                Ok((r, expr.ty.clone()))
            }
            ExprKind::Index(base, idx) => {
                let (base_addr, base_ty) = self.base_address(base)?;
                let elem = base_ty.pointee().cloned().unwrap_or(Type::int());
                let ir = self.lower_expr(idx)?;
                let ir = self.convert(ir, idx.ty.clone(), Type::long());
                let scaled = self.scale_by_elem_size(ir, elem.size_with_structs(self.structs));
                let sum = self.func.new_reg();
                self.emit(Instr::Binop {
                    dest: sum,
                    op: BinOp::Add,
                    lhs: base_addr,
                    rhs: scaled,
                    ty: Type::Pointer(Box::new(elem)),
                });
                Ok((sum, expr.ty.clone()))
            }
            ExprKind::Member { base, name, arrow } => {
                let (base_addr, base_ty) = if *arrow {
                    let r = self.lower_expr(base)?;
                    (r, base.ty.clone())
                } else {
                    self.lower_addr(base)?
                };
                let sname = match base_ty.pointee().unwrap_or(&base_ty) {
                    Type::Struct(n) => n.clone(),
                    _ => anyhow::bail!("member access on non-struct"),
                };
                let off = self
                    .structs
                    .get(&sname)
                    .and_then(|d| d.field(name))
                    .map(|(o, _)| o)
                    .unwrap_or(0);
                let addr = self.offset_addr(base_addr, off as i128);
                Ok((addr, expr.ty.clone()))
            }
            ExprKind::Unary(UnOp::AddrOf, inner) => {
                let (addr, ty) = self.lower_addr(inner)?;
                Ok((addr, Type::ptr_to(ty)))
            }
            _ => anyhow::bail!("cannot materialize address of {:?}", expr.kind.kind_desc()),
        }
    }

    /// Address of an index base: arrays yield storage address, pointers
    /// evaluate to their value.
    fn base_address(&mut self, base: &Expr) -> anyhow::Result<(Reg, Type)> {
        match (&base.kind, &base.ty) {
            (ExprKind::Var(_), Type::Array(..)) => self.lower_addr(base),
            _ => self.lower_expr(base).map(|r| (r, base.ty.clone())),
        }
    }

    fn offset_addr(&mut self, addr: Reg, offset: i128) -> Reg {
        if offset == 0 {
            return addr;
        }
        let o = self.const_reg(offset, Type::long());
        let d = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: d,
            op: BinOp::Add,
            lhs: addr,
            rhs: o,
            ty: Type::void_ptr(),
        });
        d
    }

    fn scale_by_elem_size(&mut self, idx: Reg, elem_size: u64) -> Reg {
        if elem_size <= 1 {
            return idx;
        }
        let sc = self.const_reg(elem_size as i128, Type::long());
        let d = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: d,
            op: BinOp::Mul,
            lhs: idx,
            rhs: sc,
            ty: Type::long(),
        });
        d
    }

    // ---- bit-fields ------------------------------------------------------

    /// Bit-field info for the member `base.name`, when it is one.
    /// Unions and structs share the same field table; the field's
    /// `offset` is the byte offset of its storage unit.
    fn member_bitfield(&self, base: &Expr, name: &str) -> Option<BitfieldLayout> {
        let bt = &base.ty;
        let sname = match bt.pointee().unwrap_or(bt) {
            Type::Struct(n) => n.clone(),
            _ => return None,
        };
        let def = self.structs.get(&sname)?;
        def.fields
            .iter()
            .find(|f| f.name == name)?
            .bitfield
            .filter(|b| b.width > 0)
    }

    /// Read a bit-field: load the storage unit, shift the field down
    /// and mask. The mask after the shift makes the result identical
    /// for arithmetic or logical `>>`. Signed fields are sign-extended
    /// with the bias trick (`v - ((v >> (w-1)) & 1) << w`), which stays
    /// correct under either shift semantics.
    fn load_bitfield(&mut self, addr: Reg, bf: BitfieldLayout, ty: &Type) -> Reg {
        let raw = self.func.new_reg();
        self.emit(Instr::Load {
            dest: raw,
            addr,
            offset: 0,
            ty: ty.clone(),
        });
        let mut val = raw;
        if bf.lsb > 0 {
            let lsb = self.const_reg(bf.lsb as i128, ty.clone());
            let sh = self.func.new_reg();
            self.emit(Instr::Binop {
                dest: sh,
                op: BinOp::Shr,
                lhs: raw,
                rhs: lsb,
                ty: ty.clone(),
            });
            val = sh;
        }
        let m = self.const_reg(field_mask(bf.width, ty), ty.clone());
        let masked = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: masked,
            op: BinOp::BitAnd,
            lhs: val,
            rhs: m,
            ty: ty.clone(),
        });
        if bf.signed {
            self.sign_extend(masked, bf.width, ty)
        } else {
            masked
        }
    }

    /// Sign-extend the low `width` bits of `v` (of type `ty`).
    fn sign_extend(&mut self, v: Reg, width: u32, ty: &Type) -> Reg {
        let w1 = self.const_reg((width - 1) as i128, ty.clone());
        let t = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: t,
            op: BinOp::Shr,
            lhs: v,
            rhs: w1,
            ty: ty.clone(),
        });
        let one = self.const_reg(1, ty.clone());
        let bit = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: bit,
            op: BinOp::BitAnd,
            lhs: t,
            rhs: one,
            ty: ty.clone(),
        });
        let w = self.const_reg(width as i128, ty.clone());
        let adj = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: adj,
            op: BinOp::Shl,
            lhs: bit,
            rhs: w,
            ty: ty.clone(),
        });
        let d = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: d,
            op: BinOp::Sub,
            lhs: v,
            rhs: adj,
            ty: ty.clone(),
        });
        d
    }

    /// Write a bit-field with a read-modify-write of its storage unit:
    /// `unit = (unit & ~field_mask<<lsb) | ((value & mask) << lsb)`.
    /// Returns the field's stored value (masked, sign-extended for
    /// signed fields) so assignment expressions yield it as in C.
    fn store_bitfield(&mut self, addr: Reg, ty: &Type, bf: BitfieldLayout, value: Reg) -> Reg {
        let unit_bits = ty.size() * 8;
        let old = self.func.new_reg();
        self.emit(Instr::Load {
            dest: old,
            addr,
            offset: 0,
            ty: ty.clone(),
        });
        let mask = field_mask(bf.width, ty);
        // Truncate the value to the field width.
        let m = self.const_reg(mask, ty.clone());
        let trunc = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: trunc,
            op: BinOp::BitAnd,
            lhs: value,
            rhs: m,
            ty: ty.clone(),
        });
        // Move into position.
        let placed = if bf.lsb > 0 {
            let lsb = self.const_reg(bf.lsb as i128, ty.clone());
            let p = self.func.new_reg();
            self.emit(Instr::Binop {
                dest: p,
                op: BinOp::Shl,
                lhs: trunc,
                rhs: lsb,
                ty: ty.clone(),
            });
            p
        } else {
            trunc
        };
        // Clear the old field bits and merge.
        let clear_mask = if (bf.width as u64) >= unit_bits {
            0
        } else {
            !(mask << bf.lsb) & ((1i128 << unit_bits) - 1)
        };
        let cm = self.const_reg(clear_mask, ty.clone());
        let cleared = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: cleared,
            op: BinOp::BitAnd,
            lhs: old,
            rhs: cm,
            ty: ty.clone(),
        });
        let merged = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: merged,
            op: BinOp::BitOr,
            lhs: cleared,
            rhs: placed,
            ty: ty.clone(),
        });
        self.emit(Instr::Store {
            addr,
            offset: 0,
            src: merged,
            ty: ty.clone(),
        });
        if bf.signed && (bf.width as u64) < unit_bits {
            self.sign_extend(trunc, bf.width, ty)
        } else {
            trunc
        }
    }

    // ---- expressions -----------------------------------------------------

    fn lower_expr(&mut self, expr: &Expr) -> anyhow::Result<Reg> {
        let ty = expr.ty.clone();
        match &expr.kind {
            ExprKind::IntLit(v, _) => Ok(self.const_reg(*v, ty)),
            ExprKind::StrLit(s) => {
                let label = self.intern_string(s);
                let dest = self.func.new_reg();
                self.emit(Instr::GetGlobal { dest, name: label });
                Ok(dest)
            }
            ExprKind::Var(name) => {
                if let Some(Binding::Slot(slot)) = self.lookup(name) {
                    let slot_ty = self.func.slots[slot as usize].ty.clone();
                    match slot_ty {
                        Type::Array(..) | Type::Struct(_) => {
                            // Decay aggregates to their address.
                            let (addr, _) = self.lower_addr(expr)?;
                            Ok(addr)
                        }
                        _ => {
                            let dest = self.func.new_reg();
                            self.emit(Instr::LoadLocal { dest, slot });
                            Ok(dest)
                        }
                    }
                } else if matches!(ty, Type::Array(..)) {
                    // A global array used as a value decays to its
                    // address (same rule as local aggregates).
                    let dest = self.func.new_reg();
                    self.emit(Instr::GetGlobal {
                        dest,
                        name: name.clone(),
                    });
                    Ok(dest)
                } else {
                    let dest = self.func.new_reg();
                    self.emit(Instr::LoadGlobal {
                        dest,
                        name: name.clone(),
                    });
                    Ok(dest)
                }
            }
            ExprKind::Call { callee, args } => self.lower_call(callee, args, ty),
            ExprKind::Index(..) | ExprKind::Member { .. } => {
                let (addr, val_ty) = self.lower_addr(expr)?;
                // Bit-field member read: extract the field from its
                // storage unit with shifts and masks.
                if let ExprKind::Member { base, name, .. } = &expr.kind {
                    if let Some(bf) = self.member_bitfield(base, name) {
                        return Ok(self.load_bitfield(addr, bf, &val_ty));
                    }
                }
                match val_ty {
                    Type::Array(..) | Type::Struct(_) => Ok(addr),
                    elem => {
                        let dest = self.func.new_reg();
                        self.emit(Instr::Load {
                            dest,
                            addr,
                            offset: 0,
                            ty: elem,
                        });
                        Ok(dest)
                    }
                }
            }
            ExprKind::Unary(UnOp::Neg, inner) => {
                let r = self.lower_expr(inner)?;
                let d = self.func.new_reg();
                self.emit(Instr::Unop {
                    dest: d,
                    op: crate::UnOp::Neg,
                    operand: r,
                    ty,
                });
                Ok(d)
            }
            ExprKind::Unary(UnOp::Not, inner) => {
                let r = self.lower_expr(inner)?;
                let d = self.func.new_reg();
                self.emit(Instr::Unop {
                    dest: d,
                    op: crate::UnOp::Not,
                    operand: r,
                    ty: Type::int(),
                });
                Ok(d)
            }
            ExprKind::Unary(UnOp::BitNot, inner) => {
                let r = self.lower_expr(inner)?;
                let d = self.func.new_reg();
                self.emit(Instr::Unop {
                    dest: d,
                    op: crate::UnOp::BitNot,
                    operand: r,
                    ty,
                });
                Ok(d)
            }
            ExprKind::Unary(UnOp::Deref, inner) => {
                let r = self.lower_expr(inner)?;
                match &inner.ty {
                    Type::Pointer(elem) => match &**elem {
                        Type::Array(..) | Type::Struct(_) => Ok(r),
                        e => {
                            let dest = self.func.new_reg();
                            self.emit(Instr::Load {
                                dest,
                                addr: r,
                                offset: 0,
                                ty: (*e).clone(),
                            });
                            Ok(dest)
                        }
                    },
                    _ => anyhow::bail!("deref of non-pointer"),
                }
            }
            ExprKind::Unary(UnOp::AddrOf, inner) => {
                let (addr, _) = self.lower_addr(inner)?;
                Ok(addr)
            }
            ExprKind::Postfix { op, expr: inner } => {
                let plus = matches!(op, IncDec::PostInc);
                let old = self.incdec(inner, plus, true)?;
                Ok(old)
            }
            ExprKind::PrefixIncDec { op, expr: inner } => {
                let plus = matches!(op, IncDec::Inc);
                let nv = self.incdec(inner, plus, false)?;
                Ok(nv)
            }
            ExprKind::Binary(op @ (BinOp::LAnd | BinOp::LOr), lhs, rhs) => {
                let dest = self.func.new_reg();
                let merge = self.new_block();
                self.short_circuit(*op, lhs, rhs, dest, merge)?;
                self.place(merge);
                Ok(dest)
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let lr = self.lower_expr(lhs)?;
                let rr = self.lower_expr(rhs)?;
                let (d, _) = self.emit_binop(*op, lr, lhs.ty.clone(), rr, rhs.ty.clone())?;
                let _ = ty;
                Ok(d)
            }
            ExprKind::Assign(op, lhs, rhs) => self.lower_assign(*op, lhs, rhs),
            ExprKind::Ternary(cond, then, els) => {
                let cr = self.lower_expr(cond)?;
                let dest = self.func.new_reg();
                let then_b = self.new_block();
                let else_b = self.new_block();
                self.cond_branch(cr, then_b, else_b);
                let end_b = self.new_block();
                self.place(then_b);
                let tr = self.lower_expr(then)?;
                let tr = self.convert(tr, then.ty.clone(), ty.clone());
                self.emit(Instr::Convert {
                    dest,
                    src: tr,
                    to: ty.clone(),
                });
                self.jump_to(end_b);
                self.place(else_b);
                let er = self.lower_expr(els)?;
                let er = self.convert(er, els.ty.clone(), ty.clone());
                self.emit(Instr::Convert {
                    dest,
                    src: er,
                    to: ty.clone(),
                });
                self.jump_to(end_b);
                self.place(end_b);
                Ok(dest)
            }
            ExprKind::Cast(cast_ty, inner) => {
                let r = self.lower_expr(inner)?;
                Ok(self.convert(r, inner.ty.clone(), cast_ty.clone()))
            }
            ExprKind::SizeofType(_) | ExprKind::SizeofExpr(_) => {
                anyhow::bail!("sizeof survived sema")
            }
            ExprKind::Comma(l, r) => {
                self.lower_expr(l)?;
                self.lower_expr(r)
            }
            ExprKind::InitList(_) => anyhow::bail!("initializer list outside declaration"),
        }
    }

    fn lower_call(&mut self, callee: &str, args: &[Expr], ret: Type) -> anyhow::Result<Reg> {
        let sig = self.funcs.get(callee).cloned();
        let mut arg_regs = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let ar = self.lower_expr(a)?;
            let want = sig.as_ref().and_then(|s| s.params.get(i)).cloned();
            let target_ty = match want {
                Some(w) => w,
                None => match &a.ty {
                    // Variadic default promotions.
                    Type::Int(IntKind::Char) => Type::int(),
                    t => t.clone(),
                },
            };
            let ar = self.convert(ar, a.ty.clone(), target_ty.clone());
            arg_regs.push((ar, ArgKind::Direct(target_ty)));
        }
        let dest = if ret.is_void() {
            None
        } else {
            Some(self.func.new_reg())
        };
        self.emit(Instr::Call {
            dest,
            func: callee.to_string(),
            args: arg_regs,
        });
        Ok(dest.unwrap_or_else(|| self.const_reg(0, Type::int())))
    }

    /// `++`/`--`; returns either the new (prefix) or old (postfix) value.
    fn incdec(&mut self, inner: &Expr, plus: bool, post: bool) -> anyhow::Result<Reg> {
        let ty = inner.ty.clone();
        let (old, loc) = self.load_lvalue(inner)?;
        let delta = if ty.is_pointer() {
            let one = self.const_reg(1, Type::long());
            let elem = ty.pointee().cloned().unwrap_or(Type::int());
            self.scale_by_elem_size(one, elem.size())
        } else {
            self.const_reg(1, ty.clone())
        };
        let d = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: d,
            op: if plus { BinOp::Add } else { BinOp::Sub },
            lhs: old,
            rhs: delta,
            ty: ty.clone(),
        });
        self.store_back(loc, d)?;
        Ok(if post { old } else { d })
    }

    /// Load an lvalue; also reports how to store back to it.
    fn load_lvalue(&mut self, expr: &Expr) -> anyhow::Result<(Reg, LvalueAddr)> {
        match &expr.kind {
            ExprKind::Var(name) => {
                if let Some(Binding::Slot(slot)) = self.lookup(name) {
                    match self.func.slots[slot as usize].ty {
                        Type::Array(..) | Type::Struct(_) => {
                            let (addr, ty) = self.lower_addr(expr)?;
                            Ok((addr, LvalueAddr::Mem(addr, ty)))
                        }
                        _ => {
                            let dest = self.func.new_reg();
                            self.emit(Instr::LoadLocal { dest, slot });
                            Ok((dest, LvalueAddr::Local(slot)))
                        }
                    }
                } else {
                    let dest = self.func.new_reg();
                    self.emit(Instr::LoadGlobal {
                        dest,
                        name: name.clone(),
                    });
                    Ok((dest, LvalueAddr::Global(name.clone())))
                }
            }
            _ => {
                let (addr, ty) = self.lower_addr(expr)?;
                // Bit-field member: load the field value; the store
                // side knows how to merge back into the unit.
                if let ExprKind::Member { base, name, .. } = &expr.kind {
                    if let Some(bf) = self.member_bitfield(base, name) {
                        let v = self.load_bitfield(addr, bf, &ty);
                        return Ok((v, LvalueAddr::Bit { addr, ty, bf }));
                    }
                }
                match &ty {
                    Type::Array(..) | Type::Struct(_) => Ok((addr, LvalueAddr::Mem(addr, ty))),
                    elem_ty => {
                        let dest = self.func.new_reg();
                        self.emit(Instr::Load {
                            dest,
                            addr,
                            offset: 0,
                            ty: elem_ty.clone(),
                        });
                        Ok((dest, LvalueAddr::Mem(addr, ty)))
                    }
                }
            }
        }
    }

    /// Store to an lvalue; returns the value the stored expression
    /// yields (for bit-fields, the extracted field value).
    fn store_back(&mut self, loc: LvalueAddr, value: Reg) -> anyhow::Result<Reg> {
        Ok(match loc {
            LvalueAddr::Local(slot) => {
                self.emit(Instr::StoreLocal { slot, src: value });
                value
            }
            LvalueAddr::Global(name) => {
                self.emit(Instr::StoreGlobal { name, src: value });
                value
            }
            LvalueAddr::Mem(addr, ty) => {
                self.emit(Instr::Store {
                    addr,
                    offset: 0,
                    src: value,
                    ty,
                });
                value
            }
            LvalueAddr::Bit { addr, ty, bf } => self.store_bitfield(addr, &ty, bf, value),
        })
    }

    fn lower_assign(&mut self, op: AssignOp, lhs: &Expr, rhs: &Expr) -> anyhow::Result<Reg> {
        let lhs_ty = lhs.ty.clone();

        // Struct assignment is a memory copy; yields the destination
        // address (usable as a value like in C).
        if matches!(lhs_ty, Type::Struct(_)) {
            let dst = self.lower_addr(lhs)?.0;
            let src = self.lower_addr(rhs)?.0;
            let size = self.struct_size(&lhs_ty);
            self.emit(Instr::Memcpy { dst, src, size });
            return Ok(dst);
        }

        let (old, loc) = self.load_lvalue(lhs)?;
        let rr = self.lower_expr(rhs)?;

        let stored = match op {
            AssignOp::Simple => self.convert(rr, rhs.ty.clone(), lhs_ty.clone()),
            AssignOp::Compound(bop) => {
                let (res, res_ty) =
                    self.emit_binop(bop, old, lhs_ty.clone(), rr, rhs.ty.clone())?;
                self.convert(res, res_ty, lhs_ty.clone())
            }
        };
        // For bit-fields this is the extracted field value, not the
        // whole merged storage unit.
        let stored_val = self.store_back(loc, stored)?;

        // Assignment expression evaluates to the stored value.
        let out = self.func.new_reg();
        self.emit(Instr::Convert {
            dest: out,
            src: stored_val,
            to: lhs_ty,
        });
        Ok(out)
    }

    /// Emit a binary op with C conversions; returns `(result, type)`.
    fn emit_binop(
        &mut self,
        op: BinOp,
        lr: Reg,
        lt: Type,
        rr: Reg,
        rt: Type,
    ) -> anyhow::Result<(Reg, Type)> {
        if op.is_comparison() {
            let common = common_ptr_or_arith(&lt, &rt);
            let l = self.convert(lr, lt, common.clone());
            let r = self.convert(rr, rt, common.clone());
            let d = self.func.new_reg();
            self.emit(Instr::Binop {
                dest: d,
                op,
                lhs: l,
                rhs: r,
                ty: common,
            });
            return Ok((d, Type::int()));
        }
        if matches!(op, BinOp::Shl | BinOp::Shr) {
            let d = self.func.new_reg();
            self.emit(Instr::Binop {
                dest: d,
                op,
                lhs: lr,
                rhs: rr,
                ty: lt.clone(),
            });
            return Ok((d, lt));
        }
        if lt.is_pointer() || rt.is_pointer() {
            if op != BinOp::Add && op != BinOp::Sub {
                anyhow::bail!("invalid pointer arithmetic");
            }
            // ptr ± int  |  int + ptr  |  ptr - ptr (ptrdiff)
            if lt.is_pointer() && rt.is_pointer() {
                // Difference in elements: (l - r) / sizeof
                let d = self.func.new_reg();
                self.emit(Instr::Binop {
                    dest: d,
                    op: BinOp::Sub,
                    lhs: lr,
                    rhs: rr,
                    ty: lt.clone(),
                });
                let elem = lt.pointee().cloned().unwrap_or(Type::int());
                let diff = if elem.size_with_structs(self.structs) > 1 {
                    let sc = self.const_reg(elem.size() as i128, Type::long());
                    let q = self.func.new_reg();
                    self.emit(Instr::Binop {
                        dest: q,
                        op: BinOp::Div,
                        lhs: d,
                        rhs: sc,
                        ty: Type::long(),
                    });
                    q
                } else {
                    d
                };
                return Ok((diff, Type::long()));
            }
            let ptr_result_ty = if lt.is_pointer() {
                lt.clone()
            } else {
                rt.clone()
            };
            let (ptr, ptr_ty, int, int_ty) = if lt.is_pointer() {
                (lr, lt.clone(), rr, rt)
            } else {
                (rr, rt, lr, lt)
            };
            let signed = self.convert(int, int_ty, Type::long());
            let elem = ptr_ty.pointee().cloned().unwrap_or(Type::int());
            let scaled = self.scale_by_elem_size(signed, elem.size_with_structs(self.structs));
            let d = self.func.new_reg();
            self.emit(Instr::Binop {
                dest: d,
                op: if op == BinOp::Sub {
                    BinOp::Sub
                } else {
                    BinOp::Add
                },
                lhs: ptr,
                rhs: scaled,
                ty: ptr_ty,
            });
            return Ok((d, ptr_result_ty));
        }
        // Plain integer arithmetic.
        let common = usual_arith_type(&lt, &rt);
        let l = self.convert(lr, lt, common.clone());
        let r = self.convert(rr, rt, common.clone());
        let d = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: d,
            op,
            lhs: l,
            rhs: r,
            ty: common.clone(),
        });
        Ok((d, common))
    }

    /// Short-circuit `&&`/`||`: both arms write 0/1 into `dest`.
    fn short_circuit(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        dest: Reg,
        merge: BlockId,
    ) -> anyhow::Result<()> {
        let lr = self.lower_expr(lhs)?;
        let is_and = op == BinOp::LAnd;
        let rhs_b = self.new_block();
        let short_b = self.new_block();
        if is_and {
            self.cond_branch(lr, rhs_b, short_b); // lhs!=0 -> evaluate rhs
        } else {
            self.cond_branch(lr, short_b, rhs_b); // lhs!=0 -> short-circuit true
        }
        self.place(short_b);
        let sv = self.const_reg(if is_and { 0 } else { 1 }, Type::int());
        self.emit(Instr::Convert {
            dest,
            src: sv,
            to: Type::int(),
        });
        self.jump_to(merge);
        self.place(rhs_b);
        let rr = self.lower_expr(rhs)?;
        // Normalize to 0/1 against a same-width zero.
        let rr = self.convert(rr, rhs.ty.clone(), Type::long());
        let z = self.const_reg(0, Type::long());
        let nz = self.func.new_reg();
        self.emit(Instr::Binop {
            dest: nz,
            op: BinOp::Ne,
            lhs: rr,
            rhs: z,
            ty: Type::long(),
        });
        self.emit(Instr::Convert {
            dest,
            src: nz,
            to: Type::int(),
        });
        self.jump_to(merge);
        Ok(())
    }

    fn convert(&mut self, src: Reg, from: Type, to: Type) -> Reg {
        if from == to || to == Type::Void || from == Type::Void {
            return src;
        }
        let dest = self.func.new_reg();
        self.emit(Instr::Convert { dest, src, to });
        dest
    }

    fn struct_size(&self, ty: &Type) -> u64 {
        match ty {
            Type::Struct(name) => self.structs.get(name).map(|d| d.size).unwrap_or(0),
            _ => 0,
        }
    }

    // ---- statements ------------------------------------------------------

    fn lower_stmt(&mut self, stmt: &Stmt) -> anyhow::Result<()> {
        if !self.alive() {
            return Ok(());
        }
        match &stmt.kind {
            StmtKind::Empty => {}
            StmtKind::Expr(e) => {
                let _ = self.lower_expr(e);
            }
            StmtKind::Decl { ty, name, init, .. } => {
                self.lower_decl(ty, name, init.as_ref())?;
            }
            StmtKind::If(cond, then, els) => {
                let cr = self.lower_expr(cond)?;
                let then_b = self.new_block();
                match els {
                    Some(_) => {
                        let else_b = self.new_block();
                        let end_b = self.new_block();
                        self.cond_branch(cr, then_b, else_b);
                        self.place(then_b);
                        self.lower_stmt(then)?;
                        self.jump_to(end_b);
                        self.place(else_b);
                        self.lower_stmt(els.as_ref().unwrap())?;
                        self.jump_to(end_b);
                        self.place(end_b);
                    }
                    None => {
                        let end_b = self.new_block();
                        self.cond_branch(cr, then_b, end_b);
                        self.place(then_b);
                        self.lower_stmt(then)?;
                        self.jump_to(end_b);
                        self.place(end_b);
                    }
                }
            }
            StmtKind::While(cond, body) => {
                let head = self.new_block();
                self.jump_to(head);
                self.place(head);
                let cr = self.lower_expr(cond)?;
                let body_b = self.new_block();
                let end_b = self.new_block();
                self.cond_branch(cr, body_b, end_b);
                self.place(body_b);
                self.breaks.push(end_b);
                self.continues.push(head);
                self.lower_stmt(body)?;
                self.breaks.pop();
                self.continues.pop();
                self.jump_to(head);
                self.place(end_b);
            }
            StmtKind::DoWhile(body, cond) => {
                let body_b = self.new_block();
                let bottom = self.new_block();
                let end_b = self.new_block();
                self.jump_to(body_b);
                self.place(body_b);
                self.breaks.push(end_b);
                self.continues.push(bottom);
                self.lower_stmt(body)?;
                self.breaks.pop();
                self.continues.pop();
                self.jump_to(bottom);
                self.place(bottom);
                let cr = self.lower_expr(cond)?;
                self.cond_branch(cr, body_b, end_b);
                self.place(end_b);
            }
            StmtKind::For {
                init,
                cond,
                step,
                body,
            } => {
                self.scopes.push(HashMap::new());
                if let Some(init) = init {
                    self.lower_stmt(init)?;
                }
                let head = self.new_block();
                self.jump_to(head);
                self.place(head);
                let cond_val = match cond {
                    Some(c) => self.lower_expr(c)?,
                    None => self.const_reg(1, Type::int()),
                };
                let body_b = self.new_block();
                let step_b = self.new_block();
                let end_b = self.new_block();
                self.cond_branch(cond_val, body_b, end_b);
                self.place(body_b);
                self.breaks.push(end_b);
                self.continues.push(step_b);
                self.lower_stmt(body)?;
                self.breaks.pop();
                self.continues.pop();
                self.jump_to(step_b);
                self.place(step_b);
                if let Some(step) = step {
                    let _ = self.lower_expr(step);
                }
                self.jump_to(head);
                self.place(end_b);
                self.scopes.pop();
            }
            StmtKind::Switch {
                scrutinee,
                sections,
            } => {
                let sr = self.lower_expr(scrutinee)?;
                let sty = scrutinee.ty.clone();
                let end_b = self.new_block();
                self.breaks.push(end_b);

                let body_blocks: Vec<BlockId> = sections.iter().map(|_| self.new_block()).collect();
                let mut default_target: Option<BlockId> = None;

                // Dispatch chain lives in fresh blocks; the first check
                // continues from the current block.
                let first_check = self.new_block();
                self.jump_to(first_check);
                let mut check = first_check;
                for (i, section) in sections.iter().enumerate() {
                    self.place(check);
                    for label in &section.labels {
                        match label {
                            Some(case) => {
                                let cr = self.lower_expr(case)?;
                                let cr = self.convert(cr, case.ty.clone(), sty.clone());
                                let eq = self.func.new_reg();
                                self.emit(Instr::Binop {
                                    dest: eq,
                                    op: BinOp::Eq,
                                    lhs: sr,
                                    rhs: cr,
                                    ty: sty.clone(),
                                });
                                let next = self.new_block();
                                self.cond_branch(eq, body_blocks[i], next);
                                // `cond_branch` terminated the block and
                                // moved to a fresh one; realign with
                                // `next` so the next check (or the
                                // fall-out branch) lands exactly on the
                                // block this branch targets. Without
                                // this, stacked labels (`case 1: case 2:`)
                                // branch into an empty block.
                                self.place(next);
                                check = next;
                            }
                            None => {
                                default_target = Some(body_blocks[i]);
                            }
                        }
                    }
                }
                // Fall out of the chain: default or end.
                let fallback = default_target.unwrap_or(end_b);
                self.place(check);
                self.jump_to(fallback);

                // Bodies in order; fallthrough between sections.
                for (i, section) in sections.iter().enumerate() {
                    self.place(body_blocks[i]);
                    self.scopes.push(HashMap::new());
                    for s in &section.stmts {
                        self.lower_stmt(s)?;
                    }
                    self.scopes.pop();
                    let next = if i + 1 < sections.len() {
                        body_blocks[i + 1]
                    } else {
                        end_b
                    };
                    self.jump_to(next);
                }
                self.breaks.pop();
                self.place(end_b);
            }
            StmtKind::Break => {
                if let Some(target) = self.breaks.last().copied() {
                    self.terminate(Terminator::Br(target));
                }
            }
            StmtKind::Continue => {
                if let Some(target) = self.continues.last().copied() {
                    self.terminate(Terminator::Br(target));
                }
            }
            StmtKind::Return(value) => match value {
                Some(e) if !self.func.ret.is_void() => {
                    let vr = self.lower_expr(e)?;
                    let vr = self.convert(vr, e.ty.clone(), self.func.ret.clone());
                    self.terminate(Terminator::Return(Some(vr)));
                }
                Some(e) => {
                    let _ = self.lower_expr(e)?;
                    self.terminate(Terminator::Return(None));
                }
                None => self.terminate(Terminator::Return(None)),
            },
            StmtKind::Block(stmts) => {
                self.scopes.push(HashMap::new());
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                self.scopes.pop();
            }
        }
        Ok(())
    }

    fn lower_decl(&mut self, ty: &Type, name: &str, init: Option<&Init>) -> anyhow::Result<()> {
        let slot = self.func.new_slot(name, ty.clone(), false);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), Binding::Slot(slot));
        let Some(init) = init else { return Ok(()) };
        match (init, ty) {
            (Init::Expr(expr), Type::Array(elem, len))
                if **elem == Type::char_t() && matches!(expr.kind, ExprKind::StrLit(_)) =>
            {
                if let ExprKind::StrLit(s) = &expr.kind {
                    let label = self.intern_string(s);
                    let dst = self.slot_addr(slot);
                    let src = {
                        let d = self.func.new_reg();
                        self.emit(Instr::GetGlobal {
                            dest: d,
                            name: label,
                        });
                        d
                    };
                    let n = len.unwrap_or(s.len() as u64 + 1);
                    self.emit(Instr::Memcpy { dst, src, size: n });
                }
            }
            (Init::List(items), Type::Array(elem, _)) => {
                self.zero_fill(slot, ty);
                let base = self.slot_addr(slot);
                let esz = elem.size_with_structs(self.structs);
                // Check if element is a struct - handle nested initializer lists
                if let Type::Struct(sname) = &**elem {
                    let sname = sname.clone();
                    for (i, item) in items.iter().enumerate() {
                        if let Init::List(field_items) = item {
                            let elem_base = self.offset_addr(base, (i as u64 * esz) as i128);
                            if let Some(def) = self.structs.get(&sname) {
                                for (field_item, field) in field_items.iter().zip(def.fields.iter())
                                {
                                    if let Init::Expr(e) = field_item {
                                        let vr = self.lower_expr(e)?;
                                        let vr = self.convert(vr, e.ty.clone(), field.ty.clone());
                                        self.emit(Instr::Store {
                                            addr: elem_base,
                                            offset: field.offset as i64,
                                            src: vr,
                                            ty: field.ty.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Scalar array elements
                    for (i, item) in items.iter().enumerate() {
                        if let Init::Expr(e) = item {
                            let vr = self.lower_expr(e)?;
                            let vr = self.convert(vr, e.ty.clone(), (**elem).clone());
                            self.emit(Instr::Store {
                                addr: base,
                                offset: (i as u64 * esz) as i64,
                                src: vr,
                                ty: (**elem).clone(),
                            });
                        }
                    }
                }
            }
            (Init::List(items), Type::Struct(sname)) => {
                self.zero_fill(slot, ty);
                let base = self.slot_addr(slot);
                if let Some(def) = self.structs.get(sname) {
                    // Unnamed bit-fields (padding / `:0`) take no
                    // initializer, so pair items with named fields only.
                    let named: Vec<&polyglot_core::StructField> =
                        def.fields.iter().filter(|f| !f.name.is_empty()).collect();
                    for (item, field) in items.iter().zip(named) {
                        // If field is a struct, initializer item may be a nested list
                        if let Type::Struct(nested_name) = &field.ty {
                            if let Init::List(field_items) = item {
                                let field_base = self.offset_addr(base, field.offset as i128);
                                let nested_name = nested_name.clone();
                                if let Some(nested_def) = self.structs.get(&nested_name) {
                                    for (field_item, nested_field) in
                                        field_items.iter().zip(nested_def.fields.iter())
                                    {
                                        if let Init::Expr(e) = field_item {
                                            let vr = self.lower_expr(e)?;
                                            let vr = self.convert(
                                                vr,
                                                e.ty.clone(),
                                                nested_field.ty.clone(),
                                            );
                                            self.emit(Instr::Store {
                                                addr: field_base,
                                                offset: nested_field.offset as i64,
                                                src: vr,
                                                ty: nested_field.ty.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        } else if let Init::Expr(e) = item {
                            // Scalar field (bit-fields merge into their unit).
                            let vr = self.lower_expr(e)?;
                            let vr = self.convert(vr, e.ty.clone(), field.ty.clone());
                            match field.bitfield {
                                Some(bf) if bf.width > 0 => {
                                    let faddr = self.offset_addr(base, field.offset as i128);
                                    self.store_bitfield(faddr, &field.ty, bf, vr);
                                }
                                _ => self.emit(Instr::Store {
                                    addr: base,
                                    offset: field.offset as i64,
                                    src: vr,
                                    ty: field.ty.clone(),
                                }),
                            }
                        }
                    }
                }
            }
            (Init::Expr(e), _) => {
                let vr = self.lower_expr(e)?;
                let vr = self.convert(vr, e.ty.clone(), ty.clone());
                self.emit(Instr::StoreLocal { slot, src: vr });
            }
            _ => {}
        }
        Ok(())
    }

    fn slot_addr(&mut self, slot: SlotId) -> Reg {
        self.func.slots[slot as usize].addrof = true;
        let d = self.func.new_reg();
        self.emit(Instr::AddrLocal { dest: d, slot });
        d
    }

    /// Zero-fill an aggregate slot with `memset`.
    fn zero_fill(&mut self, slot: SlotId, ty: &Type) {
        let size = match ty {
            Type::Array(elem, len) => len.unwrap_or(0) * elem.size_with_structs(self.structs),
            Type::Struct(name) => self.structs.get(name).map(|d| d.size).unwrap_or(0),
            _ => 0,
        };
        if size == 0 {
            return;
        }
        let dst = self.slot_addr(slot);
        let z = self.const_reg(0, Type::int());
        let n = self.const_reg(size as i128, Type::long());
        self.emit(Instr::Call {
            dest: None,
            func: "memset".to_string(),
            args: vec![
                (dst, ArgKind::Direct(Type::void_ptr())),
                (z, ArgKind::Direct(Type::int())),
                (n, ArgKind::Direct(Type::long())),
            ],
        });
    }
}

#[derive(Clone, Debug)]
enum LvalueAddr {
    Local(SlotId),
    Global(String),
    Mem(Reg, Type),
    /// A bit-field member: its storage unit lives at `addr` (already
    /// offset), typed `ty`; `bf` locates the bits within the unit.
    Bit {
        addr: Reg,
        ty: Type,
        bf: BitfieldLayout,
    },
}

/// Bit mask covering `width` bits of the unit type `ty` (all ones for
/// a full-width field).
fn field_mask(width: u32, ty: &Type) -> i128 {
    let unit_bits = ty.size() * 8;
    if width as u64 >= unit_bits {
        -1
    } else {
        (1i128 << width) - 1
    }
}

fn usual_arith_type(a: &Type, b: &Type) -> Type {
    match (a, b) {
        (Type::Int(IntKind::Long), _) | (_, Type::Int(IntKind::Long)) => Type::long(),
        _ => Type::int(),
    }
}

fn common_ptr_or_arith(a: &Type, b: &Type) -> Type {
    match (a, b) {
        (Type::Pointer(_), _) => a.clone(),
        (_, Type::Pointer(_)) => b.clone(),
        _ => usual_arith_type(a, b),
    }
}
