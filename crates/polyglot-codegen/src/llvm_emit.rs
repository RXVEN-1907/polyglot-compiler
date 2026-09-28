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

/// x86-64 System V classification for an aggregate passed or returned
/// by value.
///
/// The psABI splits every object into eightbytes and classes each one.
/// This backend implements the subset the frontend's type model can
/// express:
///
/// * all-INTEGER aggregates of 1..=8 bytes pass/return in one register
///   (one `i64` piece);
/// * all-INTEGER aggregates of 9..=16 bytes pass/return in two
///   registers (two `i64` pieces: RDI/RSI for arguments, RAX/RDX for
///   returns); a partial trailing eightbyte travels in the low bytes of
///   its register;
/// * anything larger than 16 bytes is MEMORY class: arguments pass
///   behind a `byval` pointer, returns behind a hidden leading `sret`
///   pointer;
/// * SSE class requires float/double members. The frontend has no
///   floating-point types, so no eightbyte can classify as SSE today;
///   if one ever does, emission bails with a named error (see
///   [`Emitter::abi_plan`]).
struct AbiPlan {
    /// MEMORY class: pass/return through a stack pointer instead of
    /// register pieces.
    memory: bool,
    /// Width in bytes of each register piece, in order (1, 2, 4 or 8).
    /// Empty for MEMORY-class aggregates.
    pieces: Vec<u64>,
}

/// LLVM-level shape of one IR parameter/argument under the ABI.
#[derive(Clone, Debug)]
struct AbiArg {
    /// Full LLVM text of the parameter type, including any ABI
    /// attribute (`i64`, `ptr byval(%struct.X) align 8`, ...).
    ty: String,
    /// The IR-level type the LLVM shape was derived from.
    ir_ty: Type,
    /// Non-empty for INTEGER-class struct arguments: the eightbyte
    /// widths, each passed as one `i64`.
    pieces: Vec<u64>,
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
    /// Actual names of the compiler-generated string-pool globals,
    /// in pool order (see [`Emitter::resolve_global`]).
    lc_names: Vec<String>,
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
        let lc_names = module
            .globals
            .iter()
            .filter(|g| g.name.starts_with(".LC"))
            .map(|g| g.name.clone())
            .collect();
        Emitter {
            module,
            structs: &module.structs,
            global_types,
            sigs,
            lc_names,
            header: String::new(),
            bodies: String::new(),
            extra_decls: Vec::new(),
            extra_decl_names: HashSet::new(),
            fn_state: None,
            memcpy_used: false,
        }
    }

    /// Resolve a global reference to its emitted name.
    ///
    /// The lowerer names interned string literals by their pool index
    /// (`.LC0`, `.LC1`, ...) but stores them positionally *after* the
    /// TU's user globals, so with user globals present a body's `.LC1`
    /// is a different object from the emitted `@.LC1`. `.LC<n>`
    /// references therefore always map onto the `<n>`-th string-pool
    /// entry in order; C identifiers cannot start with `.`, so no user
    /// global can collide with this namespace. Without user globals the
    /// mapping is the identity.
    fn resolve_global(&self, name: &str) -> String {
        if let Some(idx_str) = name.strip_prefix(".LC") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if let Some(actual) = self.lc_names.get(idx) {
                    return actual.clone();
                }
            }
        }
        name.to_string()
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
            let body = self.struct_type_body(def)?;
            let _ = writeln!(self.header, "%struct.{name} = type {body}");
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

        // Declarations, skipping names this module also defines: the
        // frontend keeps a prototype and its definition as two
        // Function entries, and LLVM rejects a `declare` alongside a
        // `define` of the same symbol.
        let defined: HashSet<&str> = self
            .module
            .funcs
            .iter()
            .filter(|f| f.is_definition)
            .map(|f| f.name.as_str())
            .collect();
        for f in &self.module.funcs {
            if !f.is_definition && !defined.contains(f.name.as_str()) {
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

    /// LLVM body of a named struct type. Plain structs map field for
    /// field (LLVM packs literal structs with the same natural
    /// alignment scheme `StructDef::layout` uses, so sizes and offsets
    /// agree). Unions and structs containing bit-fields would NOT
    /// agree — a union repeats every member at offset 0 and bit-fields
    /// share storage units, while an LLVM literal struct would lay
    /// each member out back to back — so those render as a byte array
    /// of exactly the C size; every access is byte-addressed anyway.
    fn struct_type_body(&self, def: &StructDef) -> Result<String> {
        if def.is_union || def.fields.iter().any(|f| f.bitfield.is_some()) {
            return Ok(format!("{{ [{} x i8] }}", def.size));
        }
        let mut fields = String::new();
        for (i, field) in def.fields.iter().enumerate() {
            if i > 0 {
                fields.push_str(", ");
            }
            fields.push_str(&llvm_type(&field.ty, self.structs)?);
        }
        Ok(format!("{{ {fields} }}"))
    }

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
        let mut parts: Vec<String> = Vec::new();
        let (ret, sret_attr) = self.abi_ret(&sig.ret)?;
        if let Some(attr) = &sret_attr {
            parts.push(attr.clone());
        }
        for arg in self.abi_args(&sig.params)? {
            if arg.pieces.is_empty() {
                parts.push(arg.ty);
            } else {
                // One i64 per eightbyte piece.
                for _ in &arg.pieces {
                    parts.push("i64".to_string());
                }
            }
        }
        let params = parts.join(", ");
        let va = if sig.variadic {
            if params.is_empty() {
                "..."
            } else {
                ", ..."
            }
        } else {
            ""
        };
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
        // The lowerer sizes globals with a table-less `Type::size()`,
        // which reports 0 (clamped to 1) for struct types; the struct
        // table knows the real layout, so reconcile the object size
        // upward for aggregates with a known definition.
        let obj_size = match &g.ty {
            Type::Struct(n) => self
                .structs
                .get(n)
                .map(|def| g.size.max(def.size))
                .unwrap_or(g.size),
            _ => g.size,
        };
        let bytes = g.init.bytes(obj_size);
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

        // A global declared without an initializer is a C tentative
        // definition: emit it as a COMMON symbol so several TUs may
        // carry it and the linker merges them (a strong definition
        // from any TU overrides the common).
        let linkage = match g.init {
            GlobalInit::Zero => "common ",
            GlobalInit::Bytes(_) => "",
        };
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
            let _ = writeln!(self.header, "@{name} = {linkage}global ptr {init}, align 8");
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
                "@{name} = {linkage}global [{count} x {ety}] {init}, align {galign}",
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
                "@{name} = {linkage}global [{size} x i8] {init}, align {galign}",
                size = obj_size,
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
            "@{name} = {linkage}global {ty} {init}, align {galign}",
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

    // ---- SysV x86-64 aggregate ABI -----------------------------------------

    /// Compute the ABI plan for passing/returning `ty` by value.
    fn abi_plan(&self, ty: &Type) -> Result<AbiPlan> {
        let name = match ty {
            Type::Struct(n) => n,
            other => bail!("cannot pass {other} by value"),
        };
        let def = self
            .structs
            .get(name)
            .ok_or_else(|| anyhow!("unknown struct {name} in ABI classification"))?;
        let size = def.size;
        if size == 0 {
            bail!("struct {name} has zero size; cannot pass it by value");
        }
        if size > 16 {
            // psABI: more than sixteen bytes always has class MEMORY.
            return Ok(AbiPlan {
                memory: true,
                pieces: Vec::new(),
            });
        }
        // Classify the eightbytes. Every field type in the current
        // model (integers, pointers, arrays, nested aggregates) is
        // INTEGER class, so the register pieces are simply the
        // object's eightbytes; the trailing one may be partial.
        // Classifying per field would yield SSE for float/double
        // members, but no such type exists yet — bail loudly rather
        // than silently mis-compile if floating point ever lands.
        for field in &def.fields {
            let is_integerish =
                matches!(field.ty, Type::Int(_) | Type::Pointer(_)) || is_aggregate(&field.ty);
            if !is_integerish {
                bail!(
                    "struct {name} member `{}` has type {}, which needs SSE-class \
                     passing; the native backend does not support floating-point \
                     members yet",
                    if field.name.is_empty() {
                        "<bit-field>"
                    } else {
                        &field.name
                    },
                    field.ty
                );
            }
        }
        let mut pieces = Vec::new();
        let mut off = 0u64;
        while off < size {
            pieces.push((size - off).min(8));
            off += 8;
        }
        Ok(AbiPlan {
            memory: false,
            pieces,
        })
    }

    /// Alignment for MEMORY-class traffic (`byval` stack copies): the
    /// x86-64 stack argument granularity is 8 bytes, so the slot is
    /// never aligned below that.
    fn abi_align(&self, ty: &Type) -> Result<u64> {
        Ok(align_of(ty, self.structs).max(8))
    }

    /// Full LLVM text of a MEMORY-class argument type: a pointer with
    /// `byval` (the copy lands on the callee's stack) and its
    /// alignment as a separate attribute.
    fn byval_attr(&self, t: &Type) -> Result<String> {
        Ok(format!(
            "ptr byval({}) align {}",
            llvm_type(t, self.structs)?,
            self.abi_align(t)?
        ))
    }

    /// Full LLVM text of the hidden leading parameter type for a
    /// MEMORY-class struct return.
    fn sret_attr(&self, t: &Type) -> Result<String> {
        Ok(format!(
            "ptr sret({}) align {}",
            llvm_type(t, self.structs)?,
            align_of(t, self.structs)
        ))
    }

    /// LLVM-level shape of one IR parameter under the ABI.
    fn abi_arg(&self, t: &Type) -> Result<AbiArg> {
        if is_aggregate(t) {
            let plan = self.abi_plan(t)?;
            if plan.memory {
                Ok(AbiArg {
                    ty: self.byval_attr(t)?,
                    ir_ty: t.clone(),
                    pieces: Vec::new(),
                })
            } else {
                Ok(AbiArg {
                    ty: "i64".to_string(),
                    ir_ty: t.clone(),
                    pieces: plan.pieces,
                })
            }
        } else {
            Ok(AbiArg {
                ty: llvm_type(t, self.structs)?,
                ir_ty: t.clone(),
                pieces: Vec::new(),
            })
        }
    }

    /// LLVM-level shape of a whole parameter list.
    fn abi_args(&self, params: &[Type]) -> Result<Vec<AbiArg>> {
        params.iter().map(|t| self.abi_arg(t)).collect()
    }

    /// LLVM return type for `ret`, plus the hidden leading `sret`
    /// parameter attribute when the ABI demands one (MEMORY-class
    /// struct returns, which make the LLVM-level return type `void`).
    fn abi_ret(&self, ret: &Type) -> Result<(String, Option<String>)> {
        if is_aggregate(ret) {
            let plan = self.abi_plan(ret)?;
            if plan.memory {
                Ok(("void".to_string(), Some(self.sret_attr(ret)?)))
            } else if plan.pieces.len() == 1 {
                Ok(("i64".to_string(), None))
            } else {
                Ok(("{i64, i64}".to_string(), None))
            }
        } else {
            Ok((llvm_type(ret, self.structs)?, None))
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
                    Instr::LoadGlobal { name, .. } => {
                        let resolved = self.resolve_global(name);
                        Some(
                            self.global_types
                                .get(resolved.as_str())
                                .cloned()
                                .ok_or_else(|| {
                                    anyhow!("LoadGlobal references unknown global @{name}")
                                })?,
                        )
                    }
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

        // ABI-shaped signature: MEMORY-class struct returns grow a
        // hidden leading `sret` pointer; INTEGER-class struct params
        // arrive as one `i64` per eightbyte; MEMORY-class struct
        // params arrive as one `ptr byval`.
        let abi = self.abi_args(&f.param_tys)?;
        let (ret_ll, sret_attr) = self.abi_ret(&f.ret)?;

        let mut params: Vec<String> = Vec::new();
        if let Some(attr) = &sret_attr {
            params.push(format!("{attr} %agg"));
        }
        for (i, arg) in abi.iter().enumerate() {
            if arg.pieces.is_empty() {
                params.push(format!("{} %p{i}", arg.ty));
            } else {
                for j in 0..arg.pieces.len() {
                    params.push(format!("i64 %p{i}.{j}"));
                }
            }
        }
        let params = params.join(", ");
        let va = if f.variadic {
            if params.is_empty() {
                "..."
            } else {
                ", ..."
            }
        } else {
            ""
        };
        let _ = writeln!(
            self.bodies,
            "define {ret_ll} @{name}({params}{va}) {{",
            name = f.name
        );

        // Entry-block prologue: allocas for slots and registers, then
        // spill incoming parameters into their register homes.
        let _ = writeln!(self.bodies, "bb0:");
        for (i, slot) in f.slots.iter().enumerate() {
            let ty = llvm_type(&slot.ty, self.structs)?;
            let al = align_of(&slot.ty, self.structs);
            let _ = writeln!(self.bodies, "  %s{i}.a = alloca {ty}, align {al}");
        }
        for (i, ty) in self.fn_state.as_ref().unwrap().reg_ty.iter().enumerate() {
            let ll = llvm_type(ty, self.structs)?;
            let al = align_of(ty, self.structs);
            let _ = writeln!(self.bodies, "  %r{i}.a = alloca {ll}, align {al}");
        }
        for (i, arg) in abi.iter().enumerate() {
            if arg.pieces.is_empty() && !is_aggregate(&arg.ir_ty) {
                // Scalar: spill the incoming value as-is.
                let al = align_of(&arg.ir_ty, self.structs);
                let _ = writeln!(
                    self.bodies,
                    "  store {} %p{i}, ptr %r{i}.a, align {al}",
                    arg.ty
                );
            } else if arg.pieces.is_empty() {
                // MEMORY-class struct: the parameter is a pointer to
                // the caller's stack copy; copy it into the home.
                let ll = llvm_type(&arg.ir_ty, self.structs)?;
                let al = align_of(&arg.ir_ty, self.structs);
                let v = self.fresh();
                let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr %p{i}, align {al}");
                self.line(&format!("store {ll} {v}, ptr %r{i}.a, align {al}"));
            } else {
                // INTEGER-class struct: reassemble the eightbyte
                // pieces into the register's struct home so member
                // access sees the original bytes.
                for (j, &w) in arg.pieces.iter().enumerate() {
                    let val = if w == 8 {
                        format!("%p{i}.{j}")
                    } else {
                        let t = self.fresh();
                        let _ = writeln!(self.bodies, "  {t} = trunc i64 %p{i}.{j} to i{}", w * 8);
                        t
                    };
                    let q = if j == 0 {
                        format!("%r{i}.a")
                    } else {
                        let q = self.fresh();
                        let _ = writeln!(
                            self.bodies,
                            "  {q} = getelementptr i8, ptr %r{i}.a, i64 {}",
                            8 * j
                        );
                        q
                    };
                    let al = align_of(&arg.ir_ty, self.structs).min(w);
                    self.line(&format!("store i{} {val}, ptr {q}, align {al}", w * 8));
                }
            }
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

    /// Pointer to the by-value struct carried by register `r`:
    /// aggregate-typed registers hold the struct value in their home
    /// alloca (call results, ternary merges), so the home itself is
    /// the pointer; pointer-typed registers hold a decayed aggregate
    /// address, which is loaded.
    fn struct_ptr_of_reg(&mut self, r: Reg) -> Result<String> {
        let st = self.fn_state.as_ref().unwrap();
        let have = st.reg_ty[r as usize].clone();
        if is_aggregate(&have) {
            Ok(st.reg_addr[r as usize].clone())
        } else {
            self.load_reg(r, &Type::void_ptr())
        }
    }

    // ---- instructions ------------------------------------------------------

    fn emit_instr(&mut self, f: &Function, instr: &Instr) -> Result<()> {
        match instr {
            Instr::Const { dest, value, ty } => {
                // Aggregates can only appear as the lowerer's implicit
                // all-zero return; anything else is a named error.
                let lit = if is_aggregate(ty) {
                    if *value != 0 {
                        bail!("non-zero constant of aggregate type {ty}");
                    }
                    "zeroinitializer".to_string()
                } else {
                    const_literal(*value, ty)?
                };
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
                let ll = llvm_type(&slot_ty, self.structs)?;
                let al = align_of(&slot_ty, self.structs);
                if is_aggregate(&slot_ty) {
                    // By-value aggregate copy (parameter spill,
                    // `S s = f();`): move the LLVM-level value between
                    // the register home and the slot.
                    let src_ty = self
                        .fn_state
                        .as_ref()
                        .unwrap()
                        .reg_ty
                        .get(*src as usize)
                        .cloned()
                        .ok_or_else(|| anyhow!("StoreLocal source %r{src} out of range"))?;
                    let v = if is_aggregate(&src_ty) {
                        let src_addr =
                            self.fn_state.as_ref().unwrap().reg_addr[*src as usize].clone();
                        let v = self.fresh();
                        let _ =
                            writeln!(self.bodies, "  {v} = load {ll}, ptr {src_addr}, align {al}");
                        v
                    } else if src_ty.is_pointer() {
                        // Decayed aggregate address: copy through it.
                        let p = self.load_reg(*src, &Type::void_ptr())?;
                        let v = self.fresh();
                        let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr {p}, align {al}");
                        v
                    } else {
                        bail!("StoreLocal of aggregate slot s{slot} from scalar register %r{src}");
                    };
                    self.line(&format!("store {ll} {v}, ptr %s{slot}.a, align {al}"));
                } else {
                    let v = self.load_reg(*src, &slot_ty)?;
                    self.line(&format!("store {ll} {v}, ptr %s{slot}.a, align {al}"));
                }
            }
            Instr::AddrLocal { dest, slot } => {
                self.slot_ty(f, *slot)?; // validate
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store ptr %s{slot}.a, ptr {addr}, align 8"));
            }
            Instr::GetGlobal { dest, name } => {
                let resolved = self.resolve_global(name);
                if !self.global_types.contains_key(resolved.as_str()) {
                    bail!("GetGlobal references unknown global @{name}");
                }
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store ptr @{resolved}, ptr {addr}, align 8"));
            }
            Instr::LoadGlobal { dest, name } => {
                let resolved = self.resolve_global(name);
                if self.global_is_aggregate(&resolved) {
                    bail!("LoadGlobal of aggregate-storage global @{name} is unsupported (arrays decay to their address)");
                }
                let gty = self.global_ty(&resolved)?;
                let ll = llvm_type(&gty, self.structs)?;
                let al = align_of(&gty, self.structs);
                let v = self.fresh();
                let _ = writeln!(
                    self.bodies,
                    "  {v} = load {ll}, ptr @{resolved}, align {al}"
                );
                let addr = self.fn_state.as_ref().unwrap().reg_addr[*dest as usize].clone();
                self.line(&format!("store {ll} {v}, ptr {addr}, align {al}"));
            }
            Instr::StoreGlobal { name, src } => {
                let resolved = self.resolve_global(name);
                let gty = self.global_ty(&resolved)?;
                let v = self.load_reg(*src, &gty)?;
                let ll = llvm_type(&gty, self.structs)?;
                let al = align_of(&gty, self.structs);
                self.line(&format!("store {ll} {v}, ptr @{resolved}, align {al}"));
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
                let ll = llvm_type(ty, self.structs)?;
                let al = align_of(ty, self.structs);
                if is_aggregate(ty) {
                    // Whole-object store: the source register holds the
                    // struct value (or a decayed address to copy
                    // through), mirroring Instr::Load.
                    let src_ty = self
                        .fn_state
                        .as_ref()
                        .unwrap()
                        .reg_ty
                        .get(*src as usize)
                        .cloned()
                        .ok_or_else(|| anyhow!("Store source %r{src} out of range"))?;
                    let v = if is_aggregate(&src_ty) {
                        let src_addr =
                            self.fn_state.as_ref().unwrap().reg_addr[*src as usize].clone();
                        let v = self.fresh();
                        let _ =
                            writeln!(self.bodies, "  {v} = load {ll}, ptr {src_addr}, align {al}");
                        v
                    } else if src_ty.is_pointer() {
                        let sp = self.load_reg(*src, &Type::void_ptr())?;
                        let v = self.fresh();
                        let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr {sp}, align {al}");
                        v
                    } else {
                        bail!("Store of aggregate type {ty} from scalar register %r{src}");
                    };
                    self.line(&format!("store {ll} {v}, ptr {q}, align {al}"));
                } else {
                    let v = self.load_reg(*src, ty)?;
                    self.line(&format!("store {ll} {v}, ptr {q}, align {al}"));
                }
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
                    // Struct value materialization: the source either
                    // already holds the by-value struct (call result,
                    // ternary merge) or holds a decayed aggregate
                    // address to load through.
                    let src_ty = self
                        .fn_state
                        .as_ref()
                        .unwrap()
                        .reg_ty
                        .get(*src as usize)
                        .cloned()
                        .ok_or_else(|| anyhow!("Convert source %r{src} out of range"))?;
                    let ll = llvm_type(to, self.structs)?;
                    let al = align_of(to, self.structs);
                    let v = if is_aggregate(&src_ty) {
                        let src_addr =
                            self.fn_state.as_ref().unwrap().reg_addr[*src as usize].clone();
                        let v = self.fresh();
                        let _ =
                            writeln!(self.bodies, "  {v} = load {ll}, ptr {src_addr}, align {al}");
                        v
                    } else if src_ty.is_pointer() {
                        let p = self.load_reg(*src, &Type::void_ptr())?;
                        let v = self.fresh();
                        let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr {p}, align {al}");
                        v
                    } else {
                        bail!("cannot convert scalar register %r{src} to aggregate {to}");
                    };
                    self.store_reg(*dest, &v, to)?;
                } else {
                    let v = self.load_reg(*src, to)?;
                    self.store_reg(*dest, &v, to)?;
                }
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
        // One rendered LLVM-level argument slot: `ty` is the argument
        // text (may carry ABI attributes), `bare_ty` the plain type
        // for the call's function type (LLVM rejects attributes
        // there), and `val` the value.
        struct Slot {
            ty: String,
            bare_ty: String,
            val: String,
        }

        // Resolve the signature: module table, then builtins, then a
        // synthesized one from the observed argument types.
        let (ret, sig_params, variadic) = match self.callee_sig(func) {
            Ok(sig) => (sig.ret, Some(sig.params), sig.variadic),
            Err(_) => {
                let ret = match dest {
                    Some(_) => Type::int(),
                    None => Type::Void,
                };
                (ret, None, false)
            }
        };
        let n_fixed = sig_params.as_ref().map_or(0, |ps| ps.len());

        // Materialize each argument into its ABI slots. Structs pass
        // by value: one `i64` per eightbyte piece for INTEGER class, a
        // `ptr byval` for MEMORY class (also forced for variadic
        // positions, per the psABI's variadic MEMORY rule).
        let mut slots: Vec<Slot> = Vec::new();
        for (i, (r, kind)) in args.iter().enumerate() {
            let kind_ty = match kind {
                ArgKind::Direct(t) => t.clone(),
            };
            let ir_ty = sig_params
                .as_ref()
                .and_then(|ps| ps.get(i).cloned())
                .unwrap_or(kind_ty);
            if is_aggregate(&ir_ty) {
                let plan = self.abi_plan(&ir_ty)?;
                let va_position = variadic && i >= n_fixed;
                if plan.memory || va_position {
                    let p = self.struct_ptr_of_reg(*r)?;
                    let ty = self.byval_attr(&ir_ty)?;
                    slots.push(Slot {
                        ty,
                        bare_ty: "ptr".to_string(),
                        val: p,
                    });
                } else {
                    let p = self.struct_ptr_of_reg(*r)?;
                    for (j, &w) in plan.pieces.iter().enumerate() {
                        let q = if j == 0 {
                            p.clone()
                        } else {
                            self.gep_bytes(&p, (8 * j) as i64)?
                        };
                        let al = align_of(&ir_ty, self.structs).min(w);
                        let v = self.fresh();
                        let _ =
                            writeln!(self.bodies, "  {v} = load i{}, ptr {q}, align {al}", w * 8);
                        let z = if w == 8 {
                            v
                        } else {
                            let z = self.fresh();
                            let _ = writeln!(self.bodies, "  {z} = zext i{} {v} to i64", w * 8);
                            z
                        };
                        slots.push(Slot {
                            ty: "i64".to_string(),
                            bare_ty: "i64".to_string(),
                            val: z,
                        });
                    }
                }
            } else {
                let v = self.load_reg(*r, &ir_ty)?;
                let ty = llvm_type(&ir_ty, self.structs)?;
                slots.push(Slot {
                    bare_ty: ty.clone(),
                    ty,
                    val: v,
                });
            }
        }

        // Return ABI: MEMORY-class struct returns need a hidden
        // leading sret pointer (the dest register home, or a scratch
        // alloca when the result is discarded).
        let (ret_ll, sret_attr) = self.abi_ret(&ret)?;
        if let Some(attr) = &sret_attr {
            let home = match dest {
                Some(d) => self
                    .fn_state
                    .as_ref()
                    .unwrap()
                    .reg_addr
                    .get(d as usize)
                    .cloned()
                    .ok_or_else(|| anyhow!("call result register %r{d} out of range"))?,
                None => {
                    let ll = llvm_type(&ret, self.structs)?;
                    let al = align_of(&ret, self.structs);
                    let t = self.fresh();
                    let _ = writeln!(self.bodies, "  {t} = alloca {ll}, align {al}");
                    t
                }
            };
            slots.insert(
                0,
                Slot {
                    ty: attr.clone(),
                    bare_ty: "ptr".to_string(),
                    val: home,
                },
            );
        }

        // Unknown callees get a declaration synthesized from the
        // rendered argument types (matching the ABI shape a definition
        // would emit).
        let bare_tys: Vec<String> = slots.iter().map(|s| s.bare_ty.clone()).collect();
        if sig_params.is_none() {
            let decl = format!(
                "declare {ret_ll} @{func}({arg_tys})",
                arg_tys = bare_tys.join(", ")
            );
            if !self.extra_decl_names.contains(func) {
                self.extra_decl_names.insert(func.to_string());
                self.extra_decls.push(decl);
            }
        }

        let fn_ty = match variadic {
            true if bare_tys.is_empty() => "...".to_string(),
            true => format!("{}, ...", bare_tys.join(", ")),
            false => bare_tys.join(", "),
        };
        let argv = slots
            .iter()
            .map(|s| format!("{} {}", s.ty, s.val))
            .collect::<Vec<_>>()
            .join(", ");
        let has_result = ret_ll != "void" && dest.is_some();
        let assign = if has_result {
            format!("{} = ", self.fresh())
        } else {
            String::new()
        };
        let result_tmp = if has_result {
            let st = self.fn_state.as_ref().unwrap();
            Some(format!("%t{}", st.tmp - 1))
        } else {
            None
        };
        let _ = writeln!(
            self.bodies,
            "  {assign}call {ret_ll} ({fn_ty}) @{func}({argv})"
        );

        // Store the result into the dest register home.
        if let Some(d) = dest {
            if ret.is_void() {
                // Void callee with a dest register: ignore.
            } else if is_aggregate(&ret) {
                let plan = self.abi_plan(&ret)?;
                if plan.memory {
                    // sret: the home is the buffer, already written —
                    // unless a scratch alloca was used (discarded
                    // result), in which case there is no dest home to
                    // fill because dest was None.
                } else {
                    // `ret_ll` is the ABI-level return type ("i64" or
                    // "{i64, i64}"); the pieces extract from it (two
                    // eightbytes) or arrive plain (one) and reassemble
                    // into the register's struct home.
                    let tmp = result_tmp
                        .ok_or_else(|| anyhow!("call to @{func} produced no result value"))?;
                    for (j, &w) in plan.pieces.iter().enumerate() {
                        let ext = if plan.pieces.len() > 1 {
                            let ext = self.fresh();
                            let _ =
                                writeln!(self.bodies, "  {ext} = extractvalue {ret_ll} {tmp}, {j}");
                            ext
                        } else {
                            tmp.clone()
                        };
                        let val = if w == 8 {
                            ext
                        } else {
                            let t = self.fresh();
                            let _ = writeln!(self.bodies, "  {t} = trunc i64 {ext} to i{}", w * 8);
                            t
                        };
                        let al = align_of(&ret, self.structs).min(w);
                        let q = if j == 0 {
                            let st = self.fn_state.as_ref().unwrap();
                            st.reg_addr[d as usize].clone()
                        } else {
                            let q = self.fresh();
                            let home = {
                                let st = self.fn_state.as_ref().unwrap();
                                st.reg_addr[d as usize].clone()
                            };
                            let _ = writeln!(
                                self.bodies,
                                "  {q} = getelementptr i8, ptr {home}, i64 {}",
                                8 * j
                            );
                            q
                        };
                        self.line(&format!("store i{} {val}, ptr {q}, align {al}", w * 8));
                    }
                }
            } else {
                let tmp = result_tmp.unwrap();
                self.store_reg(d, &tmp, &ret)?;
            }
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
                } else if is_aggregate(&f.ret) {
                    // Defensive: `return;` in a struct-returning
                    // function hands back a zeroed object.
                    if self.abi_plan(&f.ret)?.memory {
                        let ll = llvm_type(&f.ret, self.structs)?;
                        let al = align_of(&f.ret, self.structs);
                        self.line(&format!("store {ll} zeroinitializer, ptr %agg, align {al}"));
                        self.line("ret void");
                    } else {
                        let pieces = self.abi_plan(&f.ret)?.pieces.len();
                        if pieces == 1 {
                            self.line("ret i64 0");
                        } else {
                            self.line("ret {i64, i64} zeroinitializer");
                        }
                    }
                } else {
                    // Defensive: `return;` in a non-void function.
                    let ll = llvm_type(&f.ret, self.structs)?;
                    self.line(&format!("ret {ll} 0"));
                }
            }
            Terminator::Return(Some(r)) => {
                if f.ret.is_void() {
                    self.line("ret void");
                } else if is_aggregate(&f.ret) {
                    // By-value struct return. The register carries the
                    // struct (its home for value registers, a decayed
                    // address otherwise); the pieces leave per the ABI.
                    let plan = self.abi_plan(&f.ret)?;
                    let p = self.struct_ptr_of_reg(*r)?;
                    let sal = align_of(&f.ret, self.structs);
                    if plan.memory {
                        // Copy the object into the hidden sret buffer.
                        let ll = llvm_type(&f.ret, self.structs)?;
                        let v = self.fresh();
                        let _ = writeln!(self.bodies, "  {v} = load {ll}, ptr {p}, align {sal}");
                        self.line(&format!("store {ll} {v}, ptr %agg, align {sal}"));
                        self.line("ret void");
                    } else if plan.pieces.len() == 1 {
                        let w = plan.pieces[0];
                        let al = sal.min(w);
                        let v = self.fresh();
                        let _ =
                            writeln!(self.bodies, "  {v} = load i{}, ptr {p}, align {al}", w * 8);
                        let z = if w == 8 {
                            v
                        } else {
                            let z = self.fresh();
                            let _ = writeln!(self.bodies, "  {z} = zext i{} {v} to i64", w * 8);
                            z
                        };
                        self.line(&format!("ret i64 {z}"));
                    } else {
                        // Two eightbytes: RAX carries the first, RDX the
                        // second — an LLVM {i64, i64} first-class return.
                        let mut iv = "undef".to_string();
                        for (j, &w) in plan.pieces.iter().enumerate() {
                            let q = if j == 0 {
                                p.clone()
                            } else {
                                self.gep_bytes(&p, (8 * j) as i64)?
                            };
                            let al = sal.min(w);
                            let v = self.fresh();
                            let _ = writeln!(
                                self.bodies,
                                "  {v} = load i{}, ptr {q}, align {al}",
                                w * 8
                            );
                            let z = if w == 8 {
                                v
                            } else {
                                let z = self.fresh();
                                let _ = writeln!(self.bodies, "  {z} = zext i{} {v} to i64", w * 8);
                                z
                            };
                            let next = self.fresh();
                            let _ = writeln!(
                                self.bodies,
                                "  {next} = insertvalue {{i64, i64}} {iv}, i64 {z}, {j}"
                            );
                            iv = next;
                        }
                        self.line(&format!("ret {{i64, i64}} {iv}"));
                    }
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
            ll.contains("@counter = common global i32 zeroinitializer, align 4"),
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
            ll.contains("call i32 (ptr, i32, ...) @printf(ptr "),
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
