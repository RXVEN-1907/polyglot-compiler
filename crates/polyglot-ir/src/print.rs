//! Textual rendering of Polyglot IR (the `polyglot emit-ir` format).

use crate::{Function, GlobalInit, Instr, Module, Terminator};
use polyglot_syntax::ast::BinOp;
use std::fmt::Write;

/// Render a whole module.
pub fn print_module(module: &Module) -> String {
    let mut out = String::new();
    out.push_str("; Polyglot IR\n");
    if !module.globals.is_empty() {
        out.push_str("\n; ---- globals ----\n");
        for g in &module.globals {
            let init = match &g.init {
                GlobalInit::Zero => "zero".to_string(),
                GlobalInit::Bytes(b) => {
                    if g.ty == polyglot_core::Type::char_t() && b.last() == Some(&0) && b.len() > 1
                    {
                        let s = String::from_utf8_lossy(&b[..b.len() - 1]);
                        format!("{:?} ({})", s, b.len())
                    } else {
                        format!("bytes[{}]", b.len())
                    }
                }
            };
            let kind = if g.name.starts_with(".LC") {
                "const"
            } else {
                "global"
            };
            let _ = writeln!(
                out,
                "{kind} @{name}: {ty} size={size} init={init}",
                name = g.name,
                ty = g.ty,
                size = g.size
            );
        }
    }
    for f in &module.funcs {
        out.push('\n');
        print_function(f, &mut out);
    }
    out
}

/// Render one function.
pub fn print_function(f: &Function, out: &mut String) {
    let params = f
        .param_tys
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{t} %r{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = write!(
        out,
        "func @{name}({params}) -> {ret}",
        name = f.name,
        ret = f.ret
    );
    if !f.is_definition {
        out.push_str(";\n");
        return;
    }
    out.push_str(" {\n");

    if !f.slots.is_empty() {
        for (i, slot) in f.slots.iter().enumerate() {
            let flags = if slot.addrof { ", addrof" } else { "" };
            let _ = writeln!(
                out,
                "    slot s{i}: {ty} \"{name}\"{flags}",
                ty = slot.ty,
                name = slot.name
            );
        }
        out.push('\n');
    }

    for (bi, block) in f.blocks.iter().enumerate() {
        let _ = writeln!(out, "bb{bi}:");
        for instr in &block.instrs {
            let _ = writeln!(out, "    {}", render_instr(instr));
        }
        match &block.term {
            Terminator::Return(Some(r)) => {
                let _ = writeln!(out, "    ret {}", reg(*r));
            }
            Terminator::Return(None) => {
                let _ = writeln!(out, "    ret void");
            }
            Terminator::Br(target) => {
                let _ = writeln!(out, "    br bb{target}");
            }
            Terminator::CondBr { cond, then, else_ } => {
                let _ = writeln!(out, "    br.cond {}, bb{then}, bb{else_}", reg(*cond));
            }
            Terminator::Unreachable => {
                let _ = writeln!(out, "    unreachable");
            }
        }
    }
    out.push_str("}\n");
}

fn reg(r: u32) -> String {
    format!("%r{r}")
}

fn render_instr(instr: &Instr) -> String {
    match instr {
        Instr::Const { dest, value, ty } => {
            format!("{dest} = const {value} : {ty}", dest = reg(*dest))
        }
        Instr::LoadLocal { dest, slot } => {
            format!("{} = load.local s{slot}", reg(*dest))
        }
        Instr::StoreLocal { slot, src } => {
            format!("store.local s{slot}, {}", reg(*src))
        }
        Instr::AddrLocal { dest, slot } => {
            format!("{} = addr.local s{slot}", reg(*dest))
        }
        Instr::GetGlobal { dest, name } => {
            format!("{} = addr.global @{name}", reg(*dest))
        }
        Instr::LoadGlobal { dest, name } => {
            format!("{} = load.global @{name}", reg(*dest))
        }
        Instr::StoreGlobal { name, src } => {
            format!("store.global @{name}, {}", reg(*src))
        }
        Instr::Load {
            dest,
            addr,
            offset,
            ty,
        } => {
            if *offset == 0 {
                format!(
                    "{dest} = load.{ty} [{addr}]",
                    dest = reg(*dest),
                    addr = reg(*addr)
                )
            } else {
                format!(
                    "{dest} = load.{ty} [{addr}+{offset}]",
                    dest = reg(*dest),
                    addr = reg(*addr)
                )
            }
        }
        Instr::Store {
            addr,
            offset,
            src,
            ty,
        } => {
            if *offset == 0 {
                format!(
                    "store.{ty} [{addr}], {src}",
                    addr = reg(*addr),
                    src = reg(*src)
                )
            } else {
                format!(
                    "store.{ty} [{addr}+{offset}], {src}",
                    addr = reg(*addr),
                    src = reg(*src)
                )
            }
        }
        Instr::Binop {
            dest,
            op,
            lhs,
            rhs,
            ty,
        } => {
            format!(
                "{dest} = {op}.{ty} {lhs}, {rhs}",
                dest = reg(*dest),
                op = binop_name(*op),
                lhs = reg(*lhs),
                rhs = reg(*rhs)
            )
        }
        Instr::Unop {
            dest,
            op,
            operand,
            ty,
        } => {
            let name = match op {
                crate::UnOp::Neg => "neg",
                crate::UnOp::Not => "lnot",
                crate::UnOp::BitNot => "bnot",
            };
            format!(
                "{dest} = {name}.{ty} {operand}",
                dest = reg(*dest),
                operand = reg(*operand)
            )
        }
        Instr::Call { dest, func, args } => {
            let a = args
                .iter()
                .map(|(r, _)| reg(*r))
                .collect::<Vec<_>>()
                .join(", ");
            match dest {
                Some(d) => format!("{} = call @{func}({a})", reg(*d)),
                None => format!("call @{func}({a})"),
            }
        }
        Instr::Convert { dest, src, to } => {
            format!("{} = convert {}, {}", reg(*dest), reg(*src), to)
        }
        Instr::Memcpy { dst, src, size } => {
            format!(
                "memcpy [{dst}], [{src}], {size}",
                dst = reg(*dst),
                src = reg(*src)
            )
        }
    }
}

fn binop_name(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "add",
        Sub => "sub",
        Mul => "mul",
        Div => "div",
        Mod => "mod",
        Shl => "shl",
        Shr => "shr",
        BitAnd => "and",
        BitXor => "xor",
        BitOr => "or",
        Eq => "cmp.eq",
        Ne => "cmp.ne",
        Lt => "cmp.lt",
        Gt => "cmp.gt",
        Le => "cmp.le",
        Ge => "cmp.ge",
        LAnd => "land",
        LOr => "lor",
    }
}
