//! Direct polyglot-ir -> LLVM 17 textual IR emission.
//!
//! Strategy: Polyglot IR registers are function-scoped, non-SSA, and may
//! even be defined in more than one block (ternary and short-circuit
//! merges write one register from two branches). The emitter therefore
//! maps *every* IR register to an alloca in the LLVM entry block and
//! lowers each def/use to explicit store/load pairs; LLVM's backend
//! reconstructs SSA from the allocas. Local slots keep their shaped
//! types (aggregates become `[N x T]` / `%struct.X` allocas).
//!
//! Emission is pure: `emit_ll` renders the module. `emit_object` shells
//! out to `llc -filetype=obj` (no clang compilation of C source) to
//! produce a real ELF object.

use anyhow::{anyhow, bail, Context, Result};
use polyglot_core::{IntKind, StructDef, Type};
use polyglot_ir::{
    ArgKind, BinOp, Function, GlobalInit, GlobalVar, Instr, Module, Reg, SlotId, Terminator,
};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Host triple baked into every emitted module.
fn host_triple() -> String {
    target_lexicon::HOST.to_string()
}

/// LLVM integer type for a polyglot integer kind. The IR's integer
/// model is signed-only (`char`/`int`/`long`), so all divisions,
/// remainders, shifts and comparisons use the signed (`s`) forms.
fn int_bits(k: IntKind) -> u32 {
    match k {
        IntKind::Char => 8,
        IntKind::Int => 32,
        IntKind::Long => 64,
    }
}

/// LLVM type string for a polyglot type (LLVM 17 opaque pointers).
fn llvm_type(ty: &Type, _structs: &HashMap<String, StructDef>) -> Result<String> {
    Ok(match ty {
        Type::Void => "void".to_string(),
        Type::Int(k) => format!("i{}", int_bits(*k)),
        Type::Pointer(_) => "ptr".to_string(),
        Type::Array(elem, len) => {
            format!("[{} x {}]", len.unwrap_or(0), llvm_type(elem, _structs)?)
        }
        Type::Struct(name) => format!("%struct.{name}"),
    })
}

fn align_of(ty: &Type, structs: &HashMap<String, StructDef>) -> u64 {
    ty.align_with_structs(structs).max(1)
}

/// Is this type an aggregate the IR passes by address?
fn is_aggregate(ty: &Type) -> bool {
    matches!(ty, Type::Array(..) | Type::Struct(_))
}

/// Render bytes as an LLVM `c"..."` constant string body.
fn escape_c_bytes(bytes: &[u8]) -> String {
    let mut s = String::from("c\"");
    for &b in bytes {
        match b {
            0x20..=0x7E if b != b'"' && b != b'\\' => s.push(b as char),
            _ => {
                let _ = write!(s, "\\{:02X}", b);
            }
        }
    }
    s.push('"');
    s
}

/// Mask an i128 constant to `bits` and print it as a signed decimal
/// LLVM literal.
fn int_literal(value: i128, bits: u32) -> String {
    let masked = match bits {
        8 => (value as i8) as i128,
        32 => (value as i32) as i128,
        _ => (value as i64) as i128,
    };
    format!("{masked}")
}

fn const_literal(value: i128, ty: &Type) -> Result<String> {
    match ty {
        Type::Pointer(_) => Ok("null".to_string()),
        Type::Int(k) => Ok(int_literal(value, int_bits(*k))),
        other => bail!("cannot build a constant of type {other}"),
    }
}

/// Signature knowledge for a callee: return type, fixed param types,
/// variadic flag.
#[derive(Clone, Debug)]
struct Sig {
    ret: Type,
    params: Vec<Type>,
    variadic: bool,
}

/// Fallback externals for callees that never appear in `module.funcs`
/// (e.g. `memset`, which the lowerer injects into sema's table but not
/// into the AST, so it never becomes an IR declaration).
fn builtin_sig(name: &str) -> Option<Sig> {
    let (ret, params, variadic) = match name {
        "memset" => (
            Type::void_ptr(),
            vec![Type::void_ptr(), Type::int(), Type::long()],
            false,
        ),
        "memcpy" => (
            Type::void_ptr(),
            vec![Type::void_ptr(), Type::void_ptr(), Type::long()],
            false,
        ),
        "printf" | "sprintf" => (Type::int(), vec![Type::char_t().into_ptr()], true),
        "puts" => (Type::int(), vec![Type::char_t().into_ptr()], false),
        "putchar" | "getchar" | "abs" => (Type::int(), vec![Type::int()], false),
        "malloc" | "calloc" => (Type::char_t().into_ptr(), vec![Type::long()], false),
        "free" | "exit" => (Type::Void, vec![Type::void_ptr()], false),
        "strlen" => (Type::long(), vec![Type::char_t().into_ptr()], false),
        _ => return None,
    };
    let params = match name {
        "calloc" => vec![Type::long(), Type::long()],
        "exit" => vec![Type::int()],
        _ => params,
    };
    Some(Sig {
        ret,
        params,
        variadic,
    })
}

trait PtrHelper {
    fn into_ptr(self) -> Type;
}
impl PtrHelper for Type {
    fn into_ptr(self) -> Type {
        Type::ptr_to(self)
    }
}

/// Per-function emission state.
struct FnState {
    /// Inferred LLVM-storage type of every IR register.
    reg_ty: Vec<Type>,
    /// LLVM alloca name for every IR register.
    reg_addr: Vec<String>,
    tmp: u32,
}

/// Whole-module emitter.
struct Emitter<'a> {
    module: &'a Module,
    structs: &'a HashMap<String, StructDef>,
    global_types: HashMap<&'a str, Type>,
    sigs: HashMap<&'a str, Sig>,
    /// Header, struct types, globals and declarations.
    header: String,
    /// Function bodies, appended after the header at assembly time.
    bodies: String,
    /// Declarations discovered while emitting calls (intrinsics and
    /// fallback externals).
    extra_decls: Vec<String>,
    extra_decl_names: HashSet<String>,
    fn_state: Option<FnState>,
    memcpy_used: bool,
}

impl<'a> Emitter<'a> {
    fn new(module: &'a Module) -> Self {
        let global_types = module
            .globals
            .iter()
            .map(|g| (g.name.as_str(), g.ty.clone()))
            .collect();
        let sigs = module
            .funcs
            .iter()
            .map(|f| {
                (
                    f.name.as_str(),
                    Sig {
                        ret: f.ret.clone(),
                        params: f.param_tys.clone(),
                        variadic: f.variadic,
                    },
                )
            })
            .collect();
        Emitter {
            module,
            structs: &module.structs,
            global_types,
            sigs,
            header: String::new(),
            bodies: String::new(),
            extra_decls: Vec::new(),
            extra_decl_names: HashSet::new(),
            fn_state: None,
            memcpy_used: false,
        }
    }

    fn run(mut self, source_name: &str) -> Result<String> {
        let _ = writeln!(
            self.header,
            "; ModuleID = '{}'",
            sanitize_comment(source_name)
        );
        let _ = writeln!(
            self.header,
            "source_filename = \"{}\"",
            sanitize_comment(source_name)
        );
        let _ = writeln!(self.header, "target triple = \"{}\"", host_triple());

        // Named struct types, in deterministic order.
        let mut names: Vec<&String> = self.module.structs.keys().collect();
        names.sort();
        for name in names {
            let def = &self.module.structs[name.as_str()];
            let mut fields = String::new();
            for (i, field) in def.fields.iter().enumerate() {
                if i > 0 {
                    fields.push_str(", ");
                }
                fields.push_str(&llvm_type(&field.ty, self.structs)?);
            }
            let _ = writeln!(self.header, "%struct.{name} = type {{ {fields} }}");
        }
        if !self.module.structs.is_empty() {
            self.header.push('\n');
        }

        for g in &self.module.globals {
            self.emit_global(g)?;
        }
        if !self.module.globals.is_empty() {
            self.header.push('\n');
        }

        for f in &self.module.funcs {
            if !f.is_definition {
                let line = self.declare_line(f)?;
                let _ = writeln!(self.header, "{line}");
            }
        }

        // Externals referenced by calls but neither defined nor declared
        // in the module: declare the known builtins (`printf`, `memset`,
        // ...) so the emitted IR is self-contained.
        for name in self.referenced_externals() {
            if let Some(sig) = builtin_sig(&name) {
                let line = self.sig_decl_line(&name, &sig)?;
                let _ = writeln!(self.header, "{line}");
            }
        }

        for f in self.module.funcs.iter().filter(|f| f.is_definition) {
            self.emit_function(f)?;
        }

        if self.memcpy_used {
            self.extra_decls
                .push("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)".to_string());
            self.extra_decl_names
                .insert("llvm.memcpy.p0.p0.i64".to_string());
        }

        // Assemble: header, struct types, globals, declarations, bodies.
        let mut out = std::mem::take(&mut self.header);
        for decl in &self.extra_decls {
            out.push_str(decl);
            out.push('\n');
        }
        if !self.extra_decls.is_empty() {
            out.push('\n');
        }
        out.push_str(&self.bodies);
        Ok(out)
    }

    // ---- module level ----------------------------------------------------

    fn declare_line(&self, f: &Function) -> Result<String> {
        self.sig_decl_line(
            &f.name,
            &Sig {
                ret: f.ret.clone(),
                params: f.param_tys.clone(),
                variadic: f.variadic,
            },
        )
    }

    fn sig_decl_line(&self, name: &str, sig: &Sig) -> Result<String> {
        let params = sig
            .params
            .iter()
            .map(|t| llvm_type(t, self.structs))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let va = if sig.variadic {
            if params.is_empty() {
                "..."
            } else {
                ", ..."
            }
        } else {
            ""
        };
        let ret = llvm_type(&sig.ret, self.structs)?;
        Ok(format!("declare {ret} @{name}({params}{va})"))
    }

    /// Callees referenced by `Call` instructions that are neither
    /// defined nor declared in the module, in first-use order.
    fn referenced_externals(&self) -> Vec<String> {
        let declared: HashSet<&str> = self
            .module
            .funcs
            .iter()
            .map(|f| f.name.as_str())
            .chain(self.extra_decl_names.iter().map(|s| s.as_str()))
            .collect();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut out: Vec<String> = Vec::new();
        for f in &self.module.funcs {
            for block in &f.blocks {
                for instr in &block.instrs {
                    if let Instr::Call { func, .. } = instr {
                        if !declared.contains(func.as_str()) && seen.insert(func.as_str()) {
                            out.push(func.clone());
                        }
                    }
                }
            }
        }
        out
    }

    /// True when a global's storage is an aggregate (rebuilt array or
    /// struct) even though the IR records a scalar/element type — i.e.
    /// a *load* of it would be wrong and it should decay to its address.
    fn global_is_aggregate(&self, name: &str) -> bool {
        if name.starts_with(".LC") {
            return false;
        }
        self.module
            .globals
            .iter()
            .find(|g| g.name == name)
            .map(|g| g.size > g.ty.size().max(1) || is_aggregate(&g.ty))
            .unwrap_or(false)
    }

    /// Emit one global. The lowerer stores array globals with their
    /// *element* type plus the total byte size, so arrays are rebuilt
    /// from `size / elem_size`; structs fall back to a byte array with
    /// the struct's alignment (all memory accesses in the IR are typed
    /// loads through `ptr`, so layout is what matters).
    fn emit_global(&mut self, g: &GlobalVar) -> Result<()> {
        let bytes = g.init.bytes(g.size);
        let elem_size = g.ty.size().max(1);
        let name = &g.name;

        if name.starts_with(".LC") {
            // Compiler-generated string literal pool entry.
            let _ = writeln!(
                self.header,
                "@{name} = private unnamed_addr constant [{size} x i8] {str}, align 1",
                size = g.size,
                str = escape_c_bytes(&bytes),
            );
            return Ok(());
        }

        let galign = align_of(&g.ty, self.structs);

        if matches!(g.ty, Type::Pointer(_)) {
            let init = if bytes.iter().all(|&b| b == 0) {
                "null".to_string()
            } else {
                let mut w = [0u8; 8];
                let n = bytes.len().min(8);
                w[..n].copy_from_slice(&bytes[..n]);
                format!("inttoptr (i64 {} to ptr)", i64::from_le_bytes(w))
            };
            let _ = writeln!(self.header, "@{name} = global ptr {init}, align 8");
            return Ok(());
        }

        if g.size > elem_size && g.size.is_multiple_of(elem_size) && !is_aggregate(&g.ty) {
            // Rebuilt array of scalars/pointers: element-wise LE decode.
            let count = g.size / elem_size;
            let init = match &g.init {
                GlobalInit::Zero => "zeroinitializer".to_string(),
                GlobalInit::Bytes(_) => {
                    let elems = (0..count)
                        .map(|i| self.scalar_from_bytes(&bytes[(i * elem_size) as usize..], &g.ty))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ");
                    format!("[{elems}]")
                }
            };
            let _ = writeln!(
                self.header,
                "@{name} = global [{count} x {ety}] {init}, align {galign}",
                ety = llvm_type(&g.ty, self.structs)?,
            );
            return Ok(());
        }

        if is_aggregate(&g.ty) || g.size != elem_size {
            // Byte-array fallback with the object's real alignment.
            let init = match &g.init {
                GlobalInit::Zero => "zeroinitializer".to_string(),
                GlobalInit::Bytes(_) => escape_c_bytes(&bytes),
            };
            let _ = writeln!(
                self.header,
                "@{name} = global [{size} x i8] {init}, align {galign}",
                size = g.size,
            );
            return Ok(());
        }

        // Single scalar object.
        let init = match &g.init {
            GlobalInit::Zero => "zeroinitializer".to_string(),
            GlobalInit::Bytes(_) => self.scalar_from_bytes(&bytes, &g.ty)?,
        };
        let _ = writeln!(
            self.header,
            "@{name} = global {ty} {init}, align {galign}",
            ty = llvm_type(&g.ty, self.structs)?,
        );
        Ok(())
    }

    /// Decode a little-endian scalar from raw bytes.
    fn scalar_from_bytes(&self, bytes: &[u8], ty: &Type) -> Result<String> {
        match ty {
            Type::Int(k) => {
                let bits = int_bits(*k);
                let n = (bits / 8) as usize;
                if bytes.len() < n {
                    bail!("global initializer too short for {ty}");
                }
                Ok(match k {
                    IntKind::Char => format!("{}", i8::from_le_bytes([bytes[0]])),
                    IntKind::Int => {
                        format!("{}", i32::from_le_bytes(bytes[..4].try_into().unwrap()))
                    }
                    IntKind::Long => {
                        format!("{}", i64::from_le_bytes(bytes[..8].try_into().unwrap()))
                    }
                })
            }
            _ => bail!("unsupported scalar global type {ty}"),
        }
    }

    // ---- function level --------------------------------------------------

    fn fresh(&mut self) -> String {
        let st = self.fn_state.as_mut().unwrap();
        let n = st.tmp;
        st.tmp += 1;
        format!("%t{n}")
    }

    fn line(&mut self, s: &str) {
        self.bodies.push_str("  ");
        self.bodies.push_str(s);
        self.bodies.push('\n');
    }

    /// Infer the storage type of every register from its definition
    /// site(s). Registers defined more than once (ternary / short-
    /// circuit merges) must agree; conflicts are a hard error.
    fn infer_reg_types(&self, f: &Function) -> Result<Vec<Type>> {
        let mut tys: Vec<Option<Type>> = vec![None; f.nregs as usize];
        for (i, t) in f.param_tys.iter().enumerate() {
            if i < tys.len() {
                tys[i] = Some(t.clone());
            }
        }
        for block in &f.blocks {
            for instr in &block.instrs {
                let Some(dest) = Function::dest(instr) else {
                    continue;
                };
                if dest as usize >= tys.len() {
                    bail!("register %r{dest} out of range (nregs = {})", f.nregs);
                }
                let cand: Option<Type> = match instr {
                    Instr::Const { ty, .. } => Some(ty.clone()),
                    Instr::LoadLocal { slot, .. } => Some(
                        f.slots
                            .get(*slot as usize)
                            .map(|s| s.ty.clone())
                            .ok_or_else(|| anyhow!("LoadLocal references unknown slot s{slot}"))?,
                    ),
                    Instr::AddrLocal { .. } | Instr::GetGlobal { .. } => Some(Type::void_ptr()),
                    Instr::LoadGlobal { name, .. } => Some(
                        self.global_types
                            .get(name.as_str())
                            .cloned()
                            .ok_or_else(|| {
                                anyhow!("LoadGlobal references unknown global @{name}")
                            })?,
                    ),
                    Instr::Load { ty, .. } => Some(ty.clone()),
                    Instr::Binop { op, ty, .. } => Some(if op.is_comparison() {
                        Type::int()
                    } else {
                        ty.clone()
                    }),
                    Instr::Unop { ty, .. } => Some(ty.clone()),
                    Instr::Call {
                        dest: Some(_),
                        func,
                        ..
                    } => Some(self.callee_sig(func)?.ret),
                    Instr::Call { dest: None, .. } => None,
                    Instr::Convert { to, .. } => Some(to.clone()),
                    Instr::StoreLocal { .. }
                    | Instr::StoreGlobal { .. }
                    | Instr::Store { .. }
                    | Instr::Memcpy { .. } => None,
                };
                if let Some(c) = cand {
                    match &mut tys[dest as usize] {
                        None => tys[dest as usize] = Some(c),
                        Some(existing) if *existing == c => {}
                        Some(existing) => bail!(
                            "register %r{dest} has conflicting inferred types ({existing} vs {c})"
                        ),
                    }
                }
            }
        }
        // Any register that escaped inference (never defined, used only
        // through allocas) gets a benign 32-bit home.
        Ok(tys
            .into_iter()
            .map(|t| t.unwrap_or_else(Type::int))
            .collect())
    }

    fn callee_sig(&self, name: &str) -> Result<Sig> {
        if let Some(sig) = self.sigs.get(name) {
            return Ok(sig.clone());
        }
        if let Some(sig) = builtin_sig(name) {
            return Ok(sig);
        }
        // Unknown external: the declaration is synthesized at the call
        // site from observed argument types (see emit_call).
        Err(anyhow!("unknown callee @{name}"))
    }

    fn emit_function(&mut self, f: &Function) -> Result<()> {
        let reg_ty = self.infer_reg_types(f)?;
        let reg_addr: Vec<String> = (0..f.nregs).map(|r| format!("%r{r}.a")).collect();
        // Slot addresses are referenced inline as `%s{slot}.a`.
        self.fn_state = Some(FnState {
            reg_ty,
            reg_addr,
            tmp: 0,
        });

        let params = f
            .param_tys
            .iter()
            .enumerate()
            .map(|(i, t)| Ok(format!("{} %p{i}", llvm_type(t, self.structs)?)))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let va = if f.variadic {
            if params.is_empty() {
                "..."
            } else {
                ", ..."
            }
        } else {
            ""
        };
        let ret = llvm_type(&f.ret, self.structs)?;
        let _ = writeln!(
            self.bodies,
            "define {ret} @{name}({params}{va}) {{",
            name = f.name
        );

        // Entry-block prologue: allocas for slots and registers, then
        // spill incoming parameters into their register homes.
        let _ = writeln!(self.bodies, "bb0:");
        for (i, slot) in f.slots.iter().enumerate() {
            if is_aggregate(&slot.ty) {
                let al = align_of(&slot.ty, self.structs);
                let _ = writeln!(
                    self.bodies,
                    "  %s{i}.a = alloca {ty}, align {al}",
                    ty = llvm_type(&slot.ty, self.structs)?,
                );
            } else {
                let ty = llvm_type(&slot.ty, self.structs)?;
                let al = align_of(&slot.ty, self.structs);
                let _ = writeln!(self.bodies, "  %s{i}.a = alloca {ty}, align {al}");
            }
        }
        for (i, ty) in self.fn_state.as_ref().unwrap().reg_ty.iter().enumerate() {
            let ll = llvm_type(ty, self.structs)?;
            let al = align_of(ty, self.structs);
            let _ = writeln!(self.bodies, "  %r{i}.a = alloca {ll}, align {al}");
        }
        for (i, ty) in f.param_tys.iter().enumerate() {
            let ll = llvm_type(ty, self.structs)?;
            let al = align_of(ty, self.structs);
            let _ = writeln!(self.bodies, "  store {ll} %p{i}, ptr %r{i}.a, align {al}");
        }

        for (bi, block) in f.blocks.iter().enumerate() {
            if bi > 0 {
                let _ = writeln!(self.bodies, "bb{bi}:");
            }
            for instr in &block.instrs {
                self.emit_instr(f, instr)?;
            }
            self.emit_terminator(f, &block.term)?;
        }
        self.bodies.push_str("}\n\n");
        self.fn_state = None;
        Ok(())
    }

    // ---- values ------------------------------------------------------------

    /// Load a register's value, coerced to `want`.
    fn load_reg(&mut self, r: Reg, want: &Type) -> Result<String> {
        let st = self.fn_state.as_ref().unwrap();
        let have = st.reg_ty[r as usize].clone();
        let addr = st.reg_addr[r as usize].clone();
        if is_aggregate(&have) {
            bail!("register %r{r} holds aggregate type {have}; aggregate values are not supported");
        }
        let ll_have = llvm_type(&have, self.structs)?;
        let v = self.fresh();
        let _ = writeln!(
            self.bodies,
            "  {v} = load {ll_have}, ptr {addr}, align {}",
            align_of(&have, self.structs)
        );
        self.coerce(&v, &have, want)
    }

    /// Reinterpret an LLVM value of type `from` as type `to`.
    fn coerce(&mut self, val: &str, from: &Type, to: &Type) -> Result<String> {
        if llvm_type(from, self.structs)? == llvm_type(to, self.structs)? {
            return Ok(val.to_string());
        }
        match (from, to) {
            (Type::Int(a), Type::Int(b)) => {
                let (wa, wb) = (int_bits(*a), int_bits(*b));
                let t = self.fresh();
                if wa < wb {
                    let _ = writeln!(self.bodies, "  {t} = sext i{wa} {val} to i{wb}");
                } else {
                    let _ = writeln!(self.bodies, "  {t} = trunc i{wa} {val} to i{wb}");
                }
                Ok(t)
            }
            (Type::Pointer(_), Type::Int(b)) => {
                let t = self.fresh();
                let _ = writeln!(
                    self.bodies,
                    "  {t} = ptrtoint ptr {val} to i{}",
                    int_bits(*b)
                );
                Ok(t)
            }
            (Type::Int(a), Type::Pointer(_)) => {
                let t = self.fresh();
                let _ = writeln!(
                    self.bodies,
                    "  {t} = inttoptr i{} {val} to ptr",
                    int_bits(*a)
                );
                Ok(t)
            }
            (Type::Pointer(_), Type::Pointer(_)) => Ok(val.to_string()),
            _ => bail!("cannot coerce {from} to {to}"),
        }
    }

    /// The register as a raw integer for pointer arithmetic (pointers
    /// are ptrtoint'ed, integers widened to i64).
    fn as_i64_for_ptr_math(&mut self, r: Reg) -> Result<String> {
        let st = self.fn_state.as_ref().unwrap();
        let have = st.reg_ty[r as usize].clone();
        match have {
            Type::Pointer(_) => {
                let p = self.load_reg(r, &Type::void_ptr())?;
                let t = self.fresh();
                let _ = writeln!(self.bodies, "  {t} = ptrtoint ptr {p} to i64");
                Ok(t)
            }
            _ => self.load_reg(r, &Type::long()),
        }
    }

    // ---- instructions ------------------------------------------------------

    fn emit_instr(&mut self, f: &Function, instr: &Instr) -> Result<()> {
        match instr {
            Instr::Const { dest, value, ty } => {
                let lit = const_literal(*value, ty)?;
                let ll = llvm_type(ty, self.structs)?;
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                let al = align_of(ty, self.structs);
                self.line(&format!("store {ll} {lit}, ptr {addr}, align {al}"));
            }
            Instr::LoadLocal { dest, slot } => {
                let slot_ty = self.slot_ty(f, *slot)?;
                if is_aggregate(&slot_ty) {
                    bail!("LoadLocal of aggregate slot s{slot} is unsupported");
                }
                let ll = llvm_type(&slot_ty, self.structs)?;
                let al = align_of(&slot_ty, self.structs);
                let v = self.fresh();
                let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr %s{slot}.a, align {al}");
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store {ll} {v}, ptr {addr}, align {al}"));
            }
            Instr::StoreLocal { slot, src } => {
                let slot_ty = self.slot_ty(f, *slot)?;
                if is_aggregate(&slot_ty) {
                    bail!("StoreLocal of aggregate slot s{slot} is unsupported");
                }
                let v = self.load_reg(*src, &slot_ty)?;
                let ll = llvm_type(&slot_ty, self.structs)?;
                let al = align_of(&slot_ty, self.structs);
                self.line(&format!("store {ll} {v}, ptr %s{slot}.a, align {al}"));
            }
            Instr::AddrLocal { dest, slot } => {
                self.slot_ty(f, *slot)?; // validate
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store ptr %s{slot}.a, ptr {addr}, align 8"));
            }
            Instr::GetGlobal { dest, name } => {
                if !self.global_types.contains_key(name.as_str()) {
                    bail!("GetGlobal references unknown global @{name}");
                }
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store ptr @{name}, ptr {addr}, align 8"));
            }
            Instr::LoadGlobal { dest, name } => {
                if self.global_is_aggregate(name) {
                    bail!("LoadGlobal of aggregate-storage global @{name} is unsupported (arrays decay to their address)");
                }
                let gty = self.global_ty(name)?;
                let ll = llvm_type(&gty, self.structs)?;
                let al = align_of(&gty, self.structs);
                let v = self.fresh();
                let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr @{name}, align {al}");
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store {ll} {v}, ptr {addr}, align {al}"));
            }
            Instr::StoreGlobal { name, src } => {
                let gty = self.global_ty(name)?;
                let v = self.load_reg(*src, &gty)?;
                let ll = llvm_type(&gty, self.structs)?;
                let al = align_of(&gty, self.structs);
                self.line(&format!("store {ll} {v}, ptr @{name}, align {al}"));
            }
            Instr::Load {
                dest,
                addr,
                offset,
                ty,
            } => {
                let p = self.load_reg(*addr, &Type::void_ptr())?;
                let q = self.gep_bytes(&p, *offset)?;
                let ll = llvm_type(ty, self.structs)?;
                let al = align_of(ty, self.structs);
                let v = self.fresh();
                let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr {q}, align {al}");
                let daddr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store {ll} {v}, ptr {daddr}, align {al}"));
            }
            Instr::Store {
                addr,
                offset,
                src,
                ty,
            } => {
                let p = self.load_reg(*addr, &Type::void_ptr())?;
                let q = self.gep_bytes(&p, *offset)?;
                let v = self.load_reg(*src, ty)?;
                let ll = llvm_type(ty, self.structs)?;
                let al = align_of(ty, self.structs);
                self.line(&format!("store {ll} {v}, ptr {q}, align {al}"));
            }
            Instr::Binop {
                dest,
                op,
                lhs,
                rhs,
                ty,
            } => {
                self.emit_binop(*dest, *op, *lhs, *rhs, ty)?;
            }
            Instr::Unop {
                dest,
                op,
                operand,
                ty,
            } => {
                let v = self.load_reg(*operand, ty)?;
                match op {
                    polyglot_ir::UnOp::Neg => {
                        let ll = llvm_type(ty, self.structs)?;
                        let n = self.fresh();
                        let _ = writeln!(self.bodies, "  {n} = sub {ll} 0, {v}");
                        self.store_reg(*dest, &n, ty)?;
                    }
                    polyglot_ir::UnOp::Not => {
                        let ll = llvm_type(ty, self.structs)?;
                        let c = self.fresh();
                        let _ = writeln!(self.bodies, "  {c} = icmp ne {ll} {v}, 0");
                        let z = self.fresh();
                        let _ = writeln!(self.bodies, "  {z} = zext i1 {c} to i32");
                        let iv = self.coerce(&z, &Type::int(), &Type::int())?;
                        self.store_reg(*dest, &iv, &Type::int())?;
                    }
                    polyglot_ir::UnOp::BitNot => {
                        let ll = llvm_type(ty, self.structs)?;
                        let n = self.fresh();
                        let _ = writeln!(self.bodies, "  {n} = xor {ll} {v}, -1");
                        self.store_reg(*dest, &n, ty)?;
                    }
                }
            }
            Instr::Call { dest, func, args } => {
                self.emit_call(*dest, func, args)?;
            }
            Instr::Convert { dest, src, to } => {
                if is_aggregate(to) {
                    bail!("Convert to aggregate type {to} is unsupported");
                }
                let st = self.fn_state.as_ref().unwrap();
                let have = st.reg_ty[*src as usize].clone();
                let v = self.load_reg(*src, to)?;
                self.store_reg(*dest, &v, to)?;
                let _ = have;
            }
            Instr::Memcpy { dst, src, size } => {
                let d = self.load_reg(*dst, &Type::void_ptr())?;
                let s = self.load_reg(*src, &Type::void_ptr())?;
                self.memcpy_used = true;
                self.line(&format!(
                    "call void @llvm.memcpy.p0.p0.i64(ptr {d}, ptr {s}, i64 {size}, i1 false)"
                ));
            }
        }
        Ok(())
    }

    fn slot_ty(&self, f: &Function, slot: SlotId) -> Result<Type> {
        f.slots
            .get(slot as usize)
            .map(|s| s.ty.clone())
            .ok_or_else(|| anyhow!("unknown slot s{slot} in @{}", f.name))
    }

    fn global_ty(&self, name: &str) -> Result<Type> {
        self.global_types
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown global @{name}"))
    }

    /// Byte-offset GEP (no-op at offset 0 with opaque pointers).
    fn gep_bytes(&mut self, ptr: &str, offset: i64) -> Result<String> {
        if offset == 0 {
            return Ok(ptr.to_string());
        }
        let q = self.fresh();
        let _ = writeln!(
            self.bodies,
            "  {q} = getelementptr i8, ptr {ptr}, i64 {offset}"
        );
        Ok(q)
    }

    fn store_reg(&mut self, dest: Reg, val: &str, ty: &Type) -> Result<()> {
        let ll = llvm_type(ty, self.structs)?;
        let addr = self.fn_state.as_ref().unwrap().reg_addr[dest as usize].clone();
        let al = align_of(ty, self.structs);
        self.line(&format!("store {ll} {val}, ptr {addr}, align {al}"));
        Ok(())
    }

    fn emit_binop(&mut self, dest: Reg, op: BinOp, lhs: Reg, rhs: Reg, ty: &Type) -> Result<()> {
        if op.is_comparison() {
            let l = self.load_reg(lhs, ty)?;
            let r = self.load_reg(rhs, ty)?;
            let pred = match op {
                BinOp::Eq => "eq",
                BinOp::Ne => "ne",
                BinOp::Lt => "slt",
                BinOp::Gt => "sgt",
                BinOp::Le => "sle",
                BinOp::Ge => "sge",
                _ => unreachable!(),
            };
            // Pointer ordering goes through integers; equality is direct.
            let (cmp_ty, l, r) = if ty.is_pointer() && !matches!(op, BinOp::Eq | BinOp::Ne) {
                let li = self.fresh();
                let _ = writeln!(self.bodies, "  {li} = ptrtoint ptr {l} to i64");
                let ri = self.fresh();
                let _ = writeln!(self.bodies, "  {ri} = ptrtoint ptr {r} to i64");
                ("i64".to_string(), li, ri)
            } else {
                (llvm_type(ty, self.structs)?, l, r)
            };
            let c = self.fresh();
            let _ = writeln!(self.bodies, "  {c} = icmp {pred} {cmp_ty} {l}, {r}");
            let z = self.fresh();
            let _ = writeln!(self.bodies, "  {z} = zext i1 {c} to i32");
            return self.store_reg(dest, &z, &Type::int());
        }

        if ty.is_pointer() {
            // ptr ± int / ptr ± ptr: byte-exact arithmetic the lowerer
            // already scaled; do it in integers and convert back.
            if !matches!(op, BinOp::Add | BinOp::Sub) {
                bail!("unsupported pointer binop {op:?}");
            }
            let l = self.as_i64_for_ptr_math(lhs)?;
            let r = self.as_i64_for_ptr_math(rhs)?;
            let s = self.fresh();
            let mnemonic = if op == BinOp::Add { "add" } else { "sub" };
            let _ = writeln!(self.bodies, "  {s} = {mnemonic} i64 {l}, {r}");
            let p = self.fresh();
            let _ = writeln!(self.bodies, "  {p} = inttoptr i64 {s} to ptr");
            return self.store_reg(dest, &p, &Type::void_ptr());
        }

        let l = self.load_reg(lhs, ty)?;
        let r = self.load_reg(rhs, ty)?;
        let mnemonic = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "sdiv",
            BinOp::Mod => "srem",
            BinOp::Shl => "shl",
            BinOp::Shr => "ashr", // the IR's integer model is signed-only
            BinOp::BitAnd => "and",
            BinOp::BitXor => "xor",
            BinOp::BitOr => "or",
            BinOp::LAnd | BinOp::LOr => {
                bail!("short-circuit operator {op:?} reached codegen as a plain Binop")
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                bail!("comparison operator {op:?} reached the arithmetic codegen path")
            }
        };
        let ll = llvm_type(ty, self.structs)?;
        let v = self.fresh();
        let _ = writeln!(self.bodies, "  {v} = {mnemonic} {ll} {l}, {r}");
        self.store_reg(dest, &v, ty)
    }

    fn emit_call(&mut self, dest: Option<Reg>, func: &str, args: &[(Reg, ArgKind)]) -> Result<()> {
        // Resolve the signature: module table, then builtins, then a
        // synthesized one from the observed argument types.
        let (ret, sig_params, variadic) = match self.callee_sig(func) {
            Ok(sig) => (sig.ret, Some(sig.params), sig.variadic),
            Err(_) => {
                let arg_tys = args
                    .iter()
                    .map(|(_, k)| match k {
                        ArgKind::Direct(t) => t.clone(),
                    })
                    .collect::<Vec<_>>();
                let ret = match dest {
                    Some(_) => Type::int(),
                    None => Type::Void,
                };
                let decl = format!(
                    "declare {ret} @{func}({params})",
                    ret = llvm_type(&ret, self.structs)?,
                    params = arg_tys
                        .iter()
                        .map(|t| llvm_type(t, self.structs))
                        .collect::<Result<Vec<_>>>()?
                        .join(", "),
                );
                if let Some(prev) = self.extra_decl_names.get(func) {
                    let _ = prev;
                    // Same-name redeclaration: keep the first; LLVM
                    // requires a single declaration. Conflicting shapes
                    // will fail in llc with a clear message.
                } else {
                    self.extra_decl_names.insert(func.to_string());
                    self.extra_decls.push(decl);
                }
                (ret, None, false)
            }
        };

        // Materialize arguments (each carries its own target type).
        let mut vals = Vec::new();
        for (r, kind) in args {
            let ty = match kind {
                ArgKind::Direct(t) => t.clone(),
            };
            vals.push((self.load_reg(*r, &ty)?, ty));
        }

        let dest = match dest {
            Some(d) if !ret.is_void() => Some(d),
            Some(_) => None, // void callee with a dest register: ignore
            None => None,
        };

        let ret_ll = llvm_type(&ret, self.structs)?;
        let assign = match dest {
            Some(_) => format!("{} = ", self.fresh()),
            None => String::new(),
        };

        if variadic {
            let fixed = sig_params
                .unwrap_or_default()
                .iter()
                .map(|t| llvm_type(t, self.structs))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            let fixed_va = if fixed.is_empty() {
                "...".to_string()
            } else {
                format!("{fixed}, ...")
            };
            let argv = vals
                .iter()
                .map(|(v, t)| Ok(format!("{} {}", llvm_type(t, self.structs)?, v)))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            let _ = writeln!(
                self.bodies,
                "  {assign}call {ret_ll} ({fixed_va}) @{func}({argv})"
            );
        } else {
            let argv = vals
                .iter()
                .map(|(v, t)| Ok(format!("{} {}", llvm_type(t, self.structs)?, v)))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            let _ = writeln!(self.bodies, "  {assign}call {ret_ll} @{func}({argv})");
        }

        if let (Some(d), false) = (dest, ret.is_void()) {
            let tmp_name = {
                let st = self.fn_state.as_ref().unwrap();
                format!("%t{}", st.tmp - 1)
            };
            self.store_reg(d, &tmp_name, &ret)?;
        }
        Ok(())
    }

    // ---- terminators -------------------------------------------------------

    fn emit_terminator(&mut self, f: &Function, term: &Terminator) -> Result<()> {
        let nblocks = f.blocks.len();
        match term {
            Terminator::Unreachable => self.line("unreachable"),
            Terminator::Return(None) => {
                if f.ret.is_void() {
                    self.line("ret void");
                } else {
                    // Defensive: `return;` in a non-void function.
                    let ll = llvm_type(&f.ret, self.structs)?;
                    self.line(&format!("ret {ll} 0"));
                }
            }
            Terminator::Return(Some(r)) => {
                if f.ret.is_void() {
                    self.line("ret void");
                } else {
                    let v = self.load_reg(*r, &f.ret)?;
                    let ll = llvm_type(&f.ret, self.structs)?;
                    self.line(&format!("ret {ll} {v}"));
                }
            }
            Terminator::Br(target) => {
                if *target as usize >= nblocks {
                    bail!("branch target bb{target} out of range in @{}", f.name);
                }
                self.line(&format!("br label %bb{target}"));
            }
            Terminator::CondBr { cond, then, else_ } => {
                if *then as usize >= nblocks || *else_ as usize >= nblocks {
                    bail!("condbr target out of range in @{}", f.name);
                }
                let st = self.fn_state.as_ref().unwrap();
                let cond_ty = st.reg_ty[*cond as usize].clone();
                let v = self.load_reg(*cond, &cond_ty)?;
                let c = if cond_ty.is_pointer() {
                    let c = self.fresh();
                    let _ = writeln!(self.bodies, "  {c} = icmp ne ptr {v}, null");
                    c
                } else {
                    let ll = llvm_type(&cond_ty, self.structs)?;
                    let c = self.fresh();
                    let _ = writeln!(self.bodies, "  {c} = icmp ne {ll} {v}, 0");
                    c
                };
                self.line(&format!("br i1 {c}, label %bb{then}, label %bb{else_}"));
            }
        }
        Ok(())
    }
}

fn sanitize_comment(s: &str) -> String {
    s.chars()
        .map(|c| if c == '"' || c == '\\' { '_' } else { c })
        .collect()
}

// ---- object emission -------------------------------------------------------

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Locate `llc`: `$POLYGLOT_LLC`, then `llc` on `PATH`, then the LLVM
/// 17 install this workspace is configured against.
pub fn find_llc() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("POLYGLOT_LLC") {
        let p = PathBuf::from(p);
        if is_executable(&p) {
            return Some(p);
        }
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let cand = dir.join("llc");
            if is_executable(&cand) {
                return Some(cand);
            }
        }
    }
    if let Ok(prefix) = std::env::var("MLIR_SYS_220_PREFIX") {
        let cand = PathBuf::from(prefix).join("bin").join("llc");
        if is_executable(&cand) {
            return Some(cand);
        }
    }
    let baked = PathBuf::from(option_env!("MLIR_SYS_220_PREFIX").unwrap_or_default());
    if !baked.as_os_str().is_empty() {
        let cand = baked.join("bin").join("llc");
        if is_executable(&cand) {
            return Some(cand);
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    match std::fs::metadata(p) {
        Ok(m) => {
            m.is_file() && {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    m.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            }
        }
        Err(_) => false,
    }
}

/// Emit `module` as LLVM IR text.
pub fn emit_ll(module: &Module) -> Result<String> {
    emit_ll_named(module, "polyglot")
}

/// Emit `module` as LLVM IR text, tagged with the source file name.
pub fn emit_ll_named(module: &Module, source_name: &str) -> Result<String> {
    Emitter::new(module).run(source_name)
}

/// Compile `module` to a native object file at `object_path` by writing
/// the LLVM IR to a temporary file and invoking `llc -filetype=obj`.
///
/// If `llc` cannot be found, the LLVM IR is written next to the
/// requested output as `<object_path>.ll` and a clear error is
/// returned. Returns the object path on success.
pub fn emit_object(module: &Module, object_path: &Path, source_name: &str) -> Result<PathBuf> {
    let ll = emit_ll_named(module, source_name)?;
    let Some(llc) = find_llc() else {
        let ll_path = object_path.with_extension("ll");
        std::fs::write(&ll_path, &ll)
            .with_context(|| format!("cannot write {}", ll_path.display()))?;
        bail!(
            "llc not found (set POLYGLOT_LLC or add llc to PATH); wrote LLVM IR to {}",
            ll_path.display()
        );
    };

    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stem = sanitize_comment(
        source_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("module")
            .trim_end_matches(".c"),
    );
    let tmp_ll =
        std::env::temp_dir().join(format!("polyglot-{}-{n}-{stem}.ll", std::process::id()));
    std::fs::write(&tmp_ll, &ll).with_context(|| format!("cannot write {}", tmp_ll.display()))?;

    if let Some(parent) = object_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }

    let run = std::process::Command::new(&llc)
        .arg("-filetype=obj")
        .arg("-relocation-model=pic")
        .arg("-o")
        .arg(object_path)
        .arg(&tmp_ll)
        .output();
    let result = match run {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(anyhow::anyhow!(
                "llc failed (exit {:?}): {}",
                out.status.code(),
                stderr.trim()
            ))
        }
        Err(e) => Err(anyhow::anyhow!(
            "cannot invoke llc at {}: {e}",
            llc.display()
        )),
    };
    let _ = std::fs::remove_file(&tmp_ll);
    result?;

    let meta = std::fs::metadata(object_path)
        .with_context(|| format!("llc produced no object at {}", object_path.display()))?;
    if meta.len() == 0 {
        bail!(
            "llc produced an empty object file at {}",
            object_path.display()
        );
    }
    Ok(object_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyglot_core::DiagnosticEngine;
    use std::path::Path;

    /// A hand-built module exercising globals, string literals, an
    /// extern variadic declaration, arithmetic and a call.
    fn hand_built_module() -> Module {
        let mut m = Module::default();

        // `int counter;`
        m.globals.push(GlobalVar {
            name: "counter".to_string(),
            ty: Type::int(),
            size: 4,
            init: GlobalInit::Zero,
        });
        // String literal pool entry: "hi\0".
        let mut bytes = b"hi".to_vec();
        bytes.push(0);
        m.globals.push(GlobalVar {
            name: ".LC0".to_string(),
            ty: Type::char_t(),
            size: bytes.len() as u64,
            init: GlobalInit::Bytes(bytes),
        });

        // declare i32 @printf(ptr, ...)
        m.funcs.push(Function {
            name: "printf".to_string(),
            ret: Type::int(),
            param_tys: vec![Type::ptr_to(Type::char_t())],
            variadic: true,
            is_definition: false,
            slots: vec![],
            blocks: vec![],
            nregs: 0,
        });

        // define i32 @add(i32 a, i32 b) { return a + b; }
        let mut add = Function {
            name: "add".to_string(),
            ret: Type::int(),
            param_tys: vec![Type::int(), Type::int()],
            variadic: false,
            is_definition: true,
            slots: vec![],
            blocks: vec![],
            nregs: 2, // params own r0, r1
        };
        add.new_block();
        let sum = add.new_reg();
        add.emit(Instr::Binop {
            dest: sum,
            op: BinOp::Add,
            lhs: 0,
            rhs: 1,
            ty: Type::int(),
        });
        add.set_terminator(Terminator::Return(Some(sum)));
        m.funcs.push(add);

        // define i32 @main() { printf(.LC0, counter); return 0; }
        let mut main = Function {
            name: "main".to_string(),
            ret: Type::int(),
            param_tys: vec![],
            variadic: false,
            is_definition: true,
            slots: vec![],
            blocks: vec![],
            nregs: 0,
        };
        main.new_block();
        let fmt = main.new_reg();
        main.emit(Instr::GetGlobal {
            dest: fmt,
            name: ".LC0".to_string(),
        });
        let c = main.new_reg();
        main.emit(Instr::LoadGlobal {
            dest: c,
            name: "counter".to_string(),
        });
        main.emit(Instr::Call {
            dest: None,
            func: "printf".to_string(),
            args: vec![
                (fmt, ArgKind::Direct(Type::ptr_to(Type::char_t()))),
                (c, ArgKind::Direct(Type::int())),
            ],
        });
        let z = main.new_reg();
        main.emit(Instr::Const {
            dest: z,
            value: 0,
            ty: Type::int(),
        });
        main.set_terminator(Terminator::Return(Some(z)));
        m.funcs.push(main);

        m
    }

    /// A hello-world module straight through the real frontend:
    /// parse_source -> check -> lower_program.
    fn hello_module() -> Module {
        let src =
            "#include <stdio.h>\nint main() { printf(\"Hello from Polyglot!\\n\"); return 0; }\n";
        let mut diags = DiagnosticEngine::new();
        let (_source, unit) =
            polyglot_syntax::parse_source(Path::new("hello.c"), src, &[], &mut diags)
                .expect("parse_source failed");
        assert!(!diags.has_errors(), "parse produced errors");
        let checked = polyglot_sema::check(unit, &mut diags).expect("sema failed");
        assert!(!diags.has_errors(), "sema produced errors");
        polyglot_ir::lower_program(checked)
    }

    #[test]
    fn emits_hand_built_module() {
        let ll = emit_ll(&hand_built_module()).unwrap();
        assert!(ll.contains("target triple = \""), "triple missing:\n{ll}");
        assert!(
            ll.contains("@.LC0 = private unnamed_addr constant [3 x i8] c\"hi\\00\", align 1"),
            "string literal wrong:\n{ll}"
        );
        assert!(
            ll.contains("@counter = global i32 zeroinitializer, align 4"),
            "scalar global wrong:\n{ll}"
        );
        assert!(
            ll.contains("declare i32 @printf(ptr, ...)"),
            "variadic declaration wrong:\n{ll}"
        );
        assert!(
            ll.contains("define i32 @add(i32 %p0, i32 %p1) {"),
            "definition signature wrong:\n{ll}"
        );
        assert!(ll.contains("= add i32 "), "binop missing:\n{ll}");
        assert!(ll.contains("ret i32 "), "ret missing:\n{ll}");
        assert!(
            ll.contains("call i32 (ptr, ...) @printf(ptr "),
            "variadic call wrong:\n{ll}"
        );
    }

    #[test]
    fn emits_lowered_hello() {
        let ll = emit_ll(&hello_module()).unwrap();
        assert!(
            ll.contains("define i32 @main"),
            "no @main definition:\n{ll}"
        );
        assert!(ll.contains("@printf"), "printf missing:\n{ll}");
        assert!(
            ll.contains("c\"Hello from Polyglot!\\0A\\00\""),
            "string literal with NUL missing:\n{ll}"
        );
    }

    #[test]
    fn compiles_hello_to_native_object() {
        let Some(_llc) = find_llc() else {
            // No llc in this environment: object emission is exercised
            // by the end-to-end driver proof instead.
            return;
        };
        let module = hello_module();
        let dir =
            std::env::temp_dir().join(format!("polyglot-codegen-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let obj = dir.join("hello.o");
        let path = emit_object(&module, &obj, "hello.c").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.len() > 16,
            "object suspiciously small: {} bytes",
            bytes.len()
        );
        assert!(bytes.starts_with(b"\x7fELF"), "not an ELF object");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
