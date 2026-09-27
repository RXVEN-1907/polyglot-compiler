//! Polyglot's intermediate representation.
//!
//! The IR is a typed, register-based, block-structured representation:
//!
//! * every [`Function`] is a list of [`Block`]s (entry block first)
//!   ending in a [`Terminator`];
//! * values live in numbered registers (`%r0`, `%r1, ...`) that are
//!   function-scoped and not in SSA form — the codegen backend maps
//!   them onto its own variable allocator;
//! * local variables live in named slots (`s0`, `s1`, ...). Scalars
//!   use cheap load/store ops; aggregates (arrays/structs) and locals
//!   whose address is taken use explicit memory addressing.
//!
//! Optimization passes in [`opt`] rewrite this IR; `print` renders it
//! for the `polyglot emit-ir` inspection command.

pub mod lower;
pub mod opt;
pub mod print;

pub use lower::lower_program;
pub use opt::{optimize_module, OptLevel, OptStats};

use polyglot_core::{StructDef, Type};
use std::collections::HashMap;

/// Re-exported so downstream backends can match on operators without
/// depending on the syntax crate.
pub use polyglot_syntax::ast::BinOp;

pub type Reg = u32;
pub type SlotId = u32;
pub type BlockId = u32;

/// A whole program: globals plus function definitions.
#[derive(Debug, Default)]
pub struct Module {
    pub globals: Vec<GlobalVar>,
    pub funcs: Vec<Function>,
    /// Struct layouts, needed by codegen for aggregate copies.
    pub structs: HashMap<String, StructDef>,
}

/// A global variable or compiler-generated constant pool entry.
#[derive(Debug)]
pub struct GlobalVar {
    pub name: String,
    pub ty: Type,
    /// Total byte size of the object.
    pub size: u64,
    pub init: GlobalInit,
}

#[derive(Debug)]
pub enum GlobalInit {
    /// All zero bytes.
    Zero,
    /// Explicit bytes (already padded to `size`).
    Bytes(Vec<u8>),
}

impl GlobalInit {
    /// Byte content of the initializer.
    pub fn bytes(&self, size: u64) -> Vec<u8> {
        match self {
            GlobalInit::Zero => vec![0u8; size as usize],
            GlobalInit::Bytes(b) => {
                let mut v = b.clone();
                v.resize(size as usize, 0);
                v
            }
        }
    }
}

/// A function definition (or declaration when `blocks` is empty).
#[derive(Debug)]
pub struct Function {
    pub name: String,
    pub ret: Type,
    pub param_tys: Vec<Type>,
    pub variadic: bool,
    pub is_definition: bool,
    /// Local slots; index == [`SlotId`].
    pub slots: Vec<Slot>,
    pub blocks: Vec<Block>,
    /// Number of registers used (registers are dense `0..nregs`).
    pub nregs: Reg,
}

#[derive(Debug)]
pub struct Slot {
    pub name: String,
    pub ty: Type,
    /// Byte size (aggregates sized; scalars use their natural size).
    pub size: u64,
    /// True once an address of this slot is taken; forces memory
    /// residency in codegen.
    pub addrof: bool,
}

#[derive(Debug, Default)]
pub struct Block {
    pub instrs: Vec<Instr>,
    pub term: Terminator,
}

/// How a call argument is passed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Direct(Type),
}

#[derive(Debug, Clone)]
pub enum Instr {
    Const {
        dest: Reg,
        value: i128,
        ty: Type,
    },
    /// Read a scalar local slot.
    LoadLocal {
        dest: Reg,
        slot: SlotId,
    },
    /// Write a scalar local slot.
    StoreLocal {
        slot: SlotId,
        src: Reg,
    },
    /// Materialize the address of a local slot.
    AddrLocal {
        dest: Reg,
        slot: SlotId,
    },
    /// Materialize the address of a global.
    GetGlobal {
        dest: Reg,
        name: String,
    },
    LoadGlobal {
        dest: Reg,
        name: String,
    },
    StoreGlobal {
        name: String,
        src: Reg,
    },
    /// Typed load from memory: `dest = *(addr + offset)` as `ty`.
    Load {
        dest: Reg,
        addr: Reg,
        offset: i64,
        ty: Type,
    },
    /// Typed store to memory: `*(addr + offset) = src`.
    Store {
        addr: Reg,
        offset: i64,
        src: Reg,
        ty: Type,
    },
    Binop {
        dest: Reg,
        op: BinOp,
        lhs: Reg,
        rhs: Reg,
        ty: Type,
    },
    Unop {
        dest: Reg,
        op: UnOp,
        operand: Reg,
        ty: Type,
    },
    Call {
        dest: Option<Reg>,
        func: String,
        args: Vec<(Reg, ArgKind)>,
    },
    /// Register copy or integer conversion into `to`.
    Convert {
        dest: Reg,
        src: Reg,
        to: Type,
    },
    /// Block memory copy (struct assignment, large initializers).
    Memcpy {
        dst: Reg,
        src: Reg,
        size: u64,
    },
}

/// Unary IR operations (subset of AST ops that survive lowering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Default)]
pub enum Terminator {
    #[default]
    Unreachable,
    Return(Option<Reg>),
    Br(BlockId),
    CondBr {
        cond: Reg,
        then: BlockId,
        else_: BlockId,
    },
}

impl Function {
    pub fn new_reg(&mut self) -> Reg {
        let r = self.nregs;
        self.nregs += 1;
        r
    }

    pub fn new_slot(&mut self, name: &str, ty: Type, addrof: bool) -> SlotId {
        let size = slot_size(&ty);
        self.slots.push(Slot {
            name: name.to_string(),
            ty,
            size,
            addrof,
        });
        self.slots.len() as SlotId - 1
    }

    pub fn new_block(&mut self) -> BlockId {
        self.blocks.push(Block::default());
        self.blocks.len() as BlockId - 1
    }

    /// Push an instruction onto the current (last) block.
    pub fn emit(&mut self, instr: Instr) {
        self.blocks.last_mut().unwrap().instrs.push(instr);
    }

    pub fn set_terminator(&mut self, term: Terminator) {
        self.blocks.last_mut().unwrap().term = term;
    }

    pub fn instr_count(&self) -> usize {
        self.blocks.iter().map(|b| b.instrs.len()).sum()
    }

    /// Registers read by an instruction.
    pub fn uses(instr: &Instr) -> Vec<Reg> {
        match instr {
            Instr::Const { .. } | Instr::GetGlobal { .. } | Instr::AddrLocal { .. } => vec![],
            Instr::LoadLocal { .. } => vec![],
            Instr::LoadGlobal { .. } => vec![],
            Instr::StoreLocal { src, .. } => vec![*src],
            Instr::StoreGlobal { src, .. } => vec![*src],
            Instr::Load { addr, .. } => vec![*addr],
            Instr::Store { addr, src, .. } => vec![*addr, *src],
            Instr::Binop { lhs, rhs, .. } => vec![*lhs, *rhs],
            Instr::Unop { operand, .. } => vec![*operand],
            Instr::Call { args, .. } => args.iter().map(|(r, _)| *r).collect(),
            Instr::Convert { src, .. } => vec![*src],
            Instr::Memcpy { dst, src, .. } => vec![*dst, *src],
        }
    }

    pub fn dest(instr: &Instr) -> Option<Reg> {
        match instr {
            Instr::Const { dest, .. }
            | Instr::LoadLocal { dest, .. }
            | Instr::AddrLocal { dest, .. }
            | Instr::GetGlobal { dest, .. }
            | Instr::LoadGlobal { dest, .. }
            | Instr::Load { dest, .. }
            | Instr::Binop { dest, .. }
            | Instr::Unop { dest, .. }
            | Instr::Convert { dest, .. } => Some(*dest),
            Instr::Call { dest, .. } => *dest,
            Instr::StoreLocal { .. }
            | Instr::StoreGlobal { .. }
            | Instr::Store { .. }
            | Instr::Memcpy { .. } => None,
        }
    }

    /// True for instructions with side effects that DCE must keep.
    pub fn has_side_effects(instr: &Instr) -> bool {
        matches!(
            instr,
            Instr::Call { .. }
                | Instr::StoreLocal { .. }
                | Instr::StoreGlobal { .. }
                | Instr::Store { .. }
                | Instr::Memcpy { .. }
        )
    }

    pub fn map_uses(instr: &mut Instr, f: &mut impl FnMut(Reg) -> Reg) {
        match instr {
            Instr::StoreLocal { src, .. } => *src = f(*src),
            Instr::StoreGlobal { src, .. } => *src = f(*src),
            Instr::Load { addr, .. } => *addr = f(*addr),
            Instr::Store { addr, src, .. } => {
                *addr = f(*addr);
                *src = f(*src);
            }
            Instr::Binop { lhs, rhs, .. } => {
                *lhs = f(*lhs);
                *rhs = f(*rhs);
            }
            Instr::Unop { operand, .. } => *operand = f(*operand),
            Instr::Call { args, .. } => {
                for (r, _) in args.iter_mut() {
                    *r = f(*r);
                }
            }
            Instr::Convert { src, .. } => *src = f(*src),
            Instr::Memcpy { dst, src, .. } => {
                *dst = f(*dst);
                *src = f(*src);
            }
            Instr::Const { .. }
            | Instr::LoadLocal { .. }
            | Instr::AddrLocal { .. }
            | Instr::GetGlobal { .. }
            | Instr::LoadGlobal { .. } => {}
        }
    }
}

impl Terminator {
    pub fn uses(&self) -> Vec<Reg> {
        match self {
            Terminator::Return(Some(r)) | Terminator::CondBr { cond: r, .. } => vec![*r],
            _ => vec![],
        }
    }

    pub fn map_uses(&mut self, f: &mut impl FnMut(Reg) -> Reg) {
        match self {
            Terminator::Return(Some(r)) => *r = f(*r),
            Terminator::CondBr { cond, .. } => *cond = f(*cond),
            _ => {}
        }
    }
}

/// Byte size of a slot from its type.
fn slot_size(ty: &Type) -> u64 {
    match ty {
        Type::Array(elem, len) => len.unwrap_or(0) * elem.size(),
        Type::Struct(_) => 0, // patched by the lowerer with layout info
        other => other.size().max(1),
    }
}
