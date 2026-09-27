//! Semantic analysis for the Polyglot C subset.
//!
//! The checker walks the parsed translation unit, resolves names,
//! annotates every expression with its type, validates assignments,
//! calls, control flow and member access, and folds `sizeof` into
//! constants. On success it returns a [`CheckedProgram`] with the
//! struct table and function signatures the lowering stage needs.

use polyglot_core::span::Span;
use polyglot_core::{Diagnostic, DiagnosticEngine, IntKind, StructDef, Type};
use polyglot_syntax::ast::*;
use std::collections::HashMap;

/// Signature of a known function.
#[derive(Clone, Debug)]
pub struct FuncSig {
    pub ret: Type,
    pub params: Vec<Type>,
    pub variadic: bool,
    pub defined: bool,
}

/// A global variable with its resolved type.
#[derive(Clone, Debug)]
pub struct GlobalInfo {
    pub name: String,
    pub ty: Type,
    pub init: Option<Init>,
    pub span: Span,
}

/// Result of successful type checking.
#[derive(Debug, Default)]
pub struct CheckedProgram {
    pub unit: TranslationUnit,
    pub structs: HashMap<String, StructDef>,
    pub funcs: HashMap<String, FuncSig>,
    pub globals: Vec<GlobalInfo>,
}

#[derive(Clone, Debug)]
struct LocalSym {
    ty: Type,
    span: Span,
}

/// Context for one function body.
struct FnCtx<'a> {
    name: String,
    ret: Type,
    scopes: Vec<HashMap<String, LocalSym>>,
    loop_depth: usize,
    switch_depth: usize,
    diags: &'a mut DiagnosticEngine,
    funcs: &'a mut HashMap<String, FuncSig>,
    structs: &'a HashMap<String, StructDef>,
    globals: &'a HashMap<String, Type>,
}

pub fn check(
    mut unit: TranslationUnit,
    diags: &mut DiagnosticEngine,
) -> anyhow::Result<CheckedProgram> {
    // eprintln!("[sema] check: {} structs, {} funcs, {} globals", unit.structs.len(), unit.funcs.len(), unit.globals.len());
    // 1. Struct table.
    let mut structs: HashMap<String, StructDef> = HashMap::new();
    for def in unit.structs.drain(..) {
        let mut def = def;
        // eprintln!("[sema] laying out struct {}", def.name);
        def.layout(&structs);
        // eprintln!("[sema] laid out {} size={} align={}", def.name, def.size, def.align);
        if structs.insert(def.name.clone(), def).is_some() {
            diags.push(Diagnostic::error("E0301", "duplicate struct definition"));
        }
    }
    // eprintln!("[sema] struct table built, {} structs", structs.len());

    // 2. Function signatures (prototypes may appear in any order).
    let mut funcs: HashMap<String, FuncSig> = HashMap::new();
    for f in &unit.funcs {
        let sig = FuncSig {
            ret: f.ret.clone(),
            params: f.params.iter().map(|p| p.ty.clone()).collect(),
            variadic: f.variadic,
            defined: f.body.is_some(),
        };
        match funcs.get(&f.name) {
            Some(prev) => {
                if prev.defined && sig.defined {
                    diags.push(
                        Diagnostic::error(
                            "E0302",
                            format!("redefinition of function `{}`", f.name),
                        )
                        .at(f.span, "second definition"),
                    );
                }
                if prev.ret != sig.ret || prev.params != sig.params {
                    diags.push(
                        Diagnostic::warning(
                            "W0303",
                            format!("conflicting declaration of `{}`", f.name),
                        )
                        .at(f.span, "here"),
                    );
                }
                if sig.defined {
                    funcs.insert(f.name.clone(), sig);
                }
            }
            None => {
                funcs.insert(f.name.clone(), sig);
            }
        }
    }

    // 3. Globals.
    let mut global_types: HashMap<String, Type> = HashMap::new();
    let mut global_infos: Vec<GlobalInfo> = Vec::new();
    for g in &unit.globals {
        let ty = resolve_global_type(g, &structs, diags);
        if global_types.contains_key(&g.name) {
            diags.push(
                Diagnostic::error("E0304", format!("redefinition of global `{}`", g.name))
                    .at(g.span, "second definition"),
            );
            continue;
        }
        global_types.insert(g.name.clone(), ty.clone());
        global_infos.push(GlobalInfo {
            name: g.name.clone(),
            ty,
            init: g.init.clone(),
            span: g.span,
        });
    }

    // 4. Bodies.
    // eprintln!("[sema] checking {} function bodies", unit.funcs.len());
    for f in &mut unit.funcs {
        // eprintln!("[sema] checking function {}", f.name);
        let Some(body) = &mut f.body else { continue };
        let mut scopes = Vec::new();
        let mut param_scope = HashMap::new();
        for p in &f.params {
            if p.name.is_empty() {
                diags.push(
                    Diagnostic::error("E0305", format!("parameter of `{}` needs a name", f.name))
                        .at(f.span, "here"),
                );
                continue;
            }
            if param_scope
                .insert(
                    p.name.clone(),
                    LocalSym {
                        ty: p.ty.clone(),
                        span: f.span,
                    },
                )
                .is_some()
            {
                diags.push(
                    Diagnostic::error("E0306", format!("duplicate parameter `{}`", p.name))
                        .at(f.span, "here"),
                );
            }
        }
        scopes.push(param_scope);
        let mut ctx = FnCtx {
            name: f.name.clone(),
            ret: f.ret.clone(),
            scopes,
            loop_depth: 0,
            switch_depth: 0,
            diags,
            funcs: &mut funcs,
            structs: &structs,
            globals: &global_types,
        };
        ctx.check_block(body);
        // Non-void function that can fall off the end: warning only.
        if !f.ret.is_void() {
            let last_returns = matches!(body.last().map(|s| &s.kind), Some(StmtKind::Return(_)));
            if !last_returns {
                ctx.diags.push(
                    Diagnostic::warning(
                        "W0307",
                        format!(
                            "function `{}` can reach end of body without returning a value",
                            f.name
                        ),
                    )
                    .at(f.span, "here"),
                );
            }
        }
    }

    if diags.has_errors() {
        anyhow::bail!("type checking failed");
    }

    Ok(CheckedProgram {
        unit,
        structs,
        funcs,
        globals: global_infos,
    })
}

/// Resolve a global's type, fixing array lengths from initializers.
fn resolve_global_type(
    g: &GlobalDef,
    structs: &HashMap<String, StructDef>,
    diags: &mut DiagnosticEngine,
) -> Type {
    let mut ty = g.ty.clone();
    if let Type::Array(elem, None) = &ty {
        let len = match &g.init {
            Some(Init::List(items)) => Some(items.len() as u64),
            Some(Init::Expr(Expr {
                kind: ExprKind::StrLit(s),
                ..
            })) if **elem == Type::char_t() => {
                Some(s.len() as u64 + 1) // NUL terminator
            }
            _ => None,
        };
        match len {
            Some(n) => ty = Type::Array(elem.clone(), Some(n)),
            None => {
                diags.push(
                    Diagnostic::error("E0308", format!("global array `{}` needs a length", g.name))
                        .at(g.span, "here"),
                );
                ty = Type::Array(elem.clone(), Some(0));
            }
        }
    }
    if let Type::Struct(name) = &ty {
        if !structs.contains_key(name) {
            diags.push(
                Diagnostic::error("E0309", format!("unknown struct `{name}`")).at(g.span, "here"),
            );
        }
    }
    ty
}

impl<'a> FnCtx<'a> {
    fn lookup(&self, name: &str) -> Option<LocalSym> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym.clone());
            }
        }
        None
    }

    fn declare(&mut self, name: &str, ty: Type, span: Span) {
        let scope = self.scopes.last_mut().unwrap();
        if let Some(prev) = scope.get(name) {
            let prev_span = prev.span;
            self.diags.push(
                Diagnostic::error(
                    "E0310",
                    format!("redeclaration of `{name}` in the same scope"),
                )
                .at(span, "here")
                .note(prev_span, "previous declaration"),
            );
        } else {
            scope.insert(name.to_string(), LocalSym { ty, span });
        }
    }

    fn check_block(&mut self, stmts: &mut [Stmt]) {
        self.scopes.push(HashMap::new());
        for s in stmts {
            self.check_stmt(s);
        }
        self.scopes.pop();
    }

    fn check_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            StmtKind::Empty | StmtKind::Break | StmtKind::Continue => {
                if matches!(stmt.kind, StmtKind::Break)
                    && self.loop_depth == 0
                    && self.switch_depth == 0
                {
                    self.diags.push(
                        Diagnostic::error("E0311", "`break` outside of loop or switch")
                            .at(stmt.span, "here"),
                    );
                }
                if matches!(stmt.kind, StmtKind::Continue) && self.loop_depth == 0 {
                    self.diags.push(
                        Diagnostic::error("E0312", "`continue` outside of a loop")
                            .at(stmt.span, "here"),
                    );
                }
            }
            StmtKind::Expr(e) => {
                self.check_expr(e);
            }
            StmtKind::Decl {
                ty,
                name,
                array_len,
                init,
            } => {
                // eprintln!("[sema] Decl {name} ty={ty:?}");
                // Resolve the declared type.
                let mut resolved = ty.clone();
                if let Type::Array(elem, None) = ty {
                    let len = match (array_len.as_ref(), init.as_ref()) {
                        (Some(e), _) => const_eval(e).map(|v| v as u64),
                        (None, Some(Init::List(items))) => Some(items.len() as u64),
                        (
                            None,
                            Some(Init::Expr(Expr {
                                kind: ExprKind::StrLit(s),
                                ..
                            })),
                        ) if **elem == Type::char_t() => Some(s.len() as u64 + 1),
                        _ => None,
                    };
                    match len {
                        Some(n) => resolved = Type::Array(elem.clone(), Some(n)),
                        None => self.diags.push(
                            Diagnostic::error(
                                "E0313",
                                format!("array `{name}` needs a constant length or initializer"),
                            )
                            .at(stmt.span, "here"),
                        ),
                    }
                }
                *ty = resolved.clone();
                // Validate initializer shapes against the type.
                if let Some(init) = init {
                    self.check_init(init, &resolved, stmt.span);
                }
                if let Some(len_expr) = array_len {
                    self.check_expr(len_expr);
                }
                self.declare(name, resolved, stmt.span);
            }
            StmtKind::If(cond, then, els) => {
                self.check_expr(cond);
                require_scalar(self, cond, "if condition");
                self.check_stmt(then);
                if let Some(els) = els {
                    self.check_stmt(els);
                }
            }
            StmtKind::While(cond, body) => {
                self.check_expr(cond);
                require_scalar(self, cond, "while condition");
                self.loop_depth += 1;
                self.check_stmt(body);
                self.loop_depth -= 1;
            }
            StmtKind::DoWhile(body, cond) => {
                self.loop_depth += 1;
                self.check_stmt(body);
                self.loop_depth -= 1;
                self.check_expr(cond);
                require_scalar(self, cond, "do-while condition");
            }
            StmtKind::For {
                init,
                cond,
                step,
                body,
            } => {
                self.scopes.push(HashMap::new());
                if let Some(init) = init {
                    self.check_stmt(init);
                }
                if let Some(cond) = cond {
                    self.check_expr(cond);
                    require_scalar(self, cond, "for condition");
                }
                if let Some(step) = step {
                    self.check_expr(step);
                }
                self.loop_depth += 1;
                self.check_stmt(body);
                self.loop_depth -= 1;
                self.scopes.pop();
            }
            StmtKind::Switch {
                scrutinee,
                sections,
            } => {
                self.check_expr(scrutinee);
                if !scrutinee.ty.is_integer() {
                    self.diags.push(
                        Diagnostic::error("E0314", "switch scrutinee must be an integer")
                            .at(scrutinee.span, "here"),
                    );
                }
                let mut seen: Vec<i128> = Vec::new();
                let mut has_default = false;
                self.switch_depth += 1;
                for section in sections {
                    for label in &mut section.labels {
                        match label {
                            Some(case) => {
                                self.check_expr(case);
                                match const_eval(case) {
                                    Some(v) => {
                                        if seen.contains(&v) {
                                            self.diags.push(
                                                Diagnostic::error(
                                                    "E0315",
                                                    format!("duplicate case value {v}"),
                                                )
                                                .at(case.span, "here"),
                                            );
                                        }
                                        seen.push(v);
                                    }
                                    None => self.diags.push(
                                        Diagnostic::error(
                                            "E0316",
                                            "case label must be an integer constant",
                                        )
                                        .at(case.span, "here"),
                                    ),
                                }
                            }
                            None => {
                                if has_default {
                                    self.diags.push(
                                        Diagnostic::error("E0317", "multiple `default` labels")
                                            .at(stmt.span, "here"),
                                    );
                                }
                                has_default = true;
                            }
                        }
                    }
                    self.scopes.push(HashMap::new());
                    for s in &mut section.stmts {
                        self.check_stmt(s);
                    }
                    self.scopes.pop();
                }
                self.switch_depth -= 1;
            }
            StmtKind::Return(value) => match value {
                Some(e) => {
                    self.check_expr(e);
                    if self.ret.is_void() {
                        self.diags.push(
                            Diagnostic::error(
                                "E0318",
                                format!("void function `{}` returns a value", self.name),
                            )
                            .at(e.span, "here"),
                        );
                    }
                }
                None => {
                    if !self.ret.is_void() {
                        self.diags.push(
                            Diagnostic::warning(
                                "W0319",
                                format!("non-void function `{}` should return a value", self.name),
                            )
                            .at(stmt.span, "here"),
                        );
                    }
                }
            },
            StmtKind::Block(stmts) => self.check_block(stmts),
        }
    }

    fn check_init(&mut self, init: &mut Init, ty: &Type, span: Span) {
        // eprintln!("[sema] check_init: init={:?}, ty={:?}", init, ty);
        match (init, ty) {
            (Init::Expr(e), Type::Struct(name)) => {
                self.check_expr(e);
                if e.ty != *ty {
                    self.diags.push(
                        Diagnostic::error(
                            "E0324",
                            format!("cannot initialize `struct {name}` from `{}`", e.ty),
                        )
                        .at(e.span, "here"),
                    );
                }
            }
            (Init::Expr(e), ty) => {
                self.check_expr(e);
                // char arrays may be initialized from string literals.
                if let (ExprKind::StrLit(s), Type::Array(elem, len)) = (&e.kind, ty) {
                    if **elem == Type::char_t() {
                        if let Some(n) = len {
                            if (s.len() as u64 + 1) > *n {
                                self.diags.push(
                                    Diagnostic::error(
                                        "E0320",
                                        "string literal is longer than the array",
                                    )
                                    .at(e.span, "here"),
                                );
                            }
                        }
                        return;
                    }
                }
                if let Type::Array(..) = ty {
                    self.diags.push(
                        Diagnostic::error("E0321", "invalid initializer for array")
                            .at(e.span, "here"),
                    );
                }
            }
            (Init::List(items), Type::Array(elem, len)) => {
                if let Some(n) = len {
                    if items.len() as u64 > *n {
                        self.diags.push(
                            Diagnostic::error("E0322", "too many initializers").at(span, "here"),
                        );
                    }
                }
                for item in items {
                    self.check_init(item, elem, span);
                }
            }
            (Init::List(items), Type::Struct(name)) => {
                let Some(def) = self.structs.get(name) else {
                    self.diags.push(
                        Diagnostic::error("E0309", format!("unknown struct `{name}`"))
                            .at(span, "here"),
                    );
                    return;
                };
                if items.len() > def.fields.len() {
                    self.diags
                        .push(Diagnostic::error("E0322", "too many initializers").at(span, "here"));
                }
                for (item, field) in items.iter_mut().zip(&def.fields) {
                    self.check_init(item, &field.ty, span);
                }
            }
            (Init::List(_), other) => {
                self.diags.push(
                    Diagnostic::error(
                        "E0323",
                        format!("braced initializer is invalid for `{other}`"),
                    )
                    .at(span, "here"),
                );
            }
        }
    }

    /// Type check an expression, filling in `expr.ty`.
    fn check_expr(&mut self, expr: &mut Expr) {
        // eprintln!("[sema] check_expr: {:?}", expr.kind);
        match &mut expr.kind {
            ExprKind::IntLit(v, suffix) => {
                expr.ty = int_lit_type(*v, *suffix);
            }
            ExprKind::StrLit(s) => {
                expr.ty = Type::ptr_to(Type::char_t());
                let _ = s;
            }
            ExprKind::Var(name) => {
                if let Some(sym) = self.lookup(name) {
                    expr.ty = sym.ty.clone();
                } else if let Some(ty) = self.globals.get(name) {
                    expr.ty = ty.clone();
                } else if self.funcs.contains_key(name) {
                    expr.ty = Type::int(); // function name in value context
                } else {
                    self.diags.push(
                        Diagnostic::error(
                            "E0325",
                            format!("use of undeclared identifier `{name}`"),
                        )
                        .at(expr.span, "here"),
                    );
                    expr.ty = Type::int();
                }
            }
            ExprKind::Call { callee, args } => {
                for a in args.iter_mut() {
                    self.check_expr(a);
                }
                let sig = match self.funcs.get(callee).cloned() {
                    Some(sig) => sig,
                    None => {
                        // Implicit declaration (C89 style): warn once.
                        self.diags.push(
                            Diagnostic::warning(
                                "W0326",
                                format!(
                                    "implicit declaration of function `{callee}`; assuming `int {callee}(...)`"
                                ),
                            )
                            .at(expr.span, "here"),
                        );
                        let sig = FuncSig {
                            ret: Type::int(),
                            params: Vec::new(),
                            variadic: true,
                            defined: false,
                        };
                        self.funcs.insert(callee.clone(), sig.clone());
                        sig
                    }
                };
                if !sig.variadic && args.len() != sig.params.len() {
                    self.diags.push(
                        Diagnostic::error(
                            "E0327",
                            format!(
                                "`{}` expects {} argument(s), got {}",
                                callee,
                                sig.params.len(),
                                args.len()
                            ),
                        )
                        .at(expr.span, "here"),
                    );
                } else if sig.variadic && args.len() < sig.params.len() {
                    self.diags.push(
                        Diagnostic::error(
                            "E0327",
                            format!(
                                "`{}` expects at least {} argument(s), got {}",
                                callee,
                                sig.params.len(),
                                args.len()
                            ),
                        )
                        .at(expr.span, "here"),
                    );
                }
                for (i, a) in args.iter_mut().enumerate() {
                    if i < sig.params.len() {
                        let want = &sig.params[i];
                        // Array arguments decay to pointers.
                        if let Type::Array(elem, _) = &a.ty {
                            a.ty = Type::ptr_to((**elem).clone());
                        }
                        if !compatible(want, &a.ty) {
                            self.diags.push(
                                Diagnostic::warning(
                                    "W0328",
                                    format!(
                                        "passing `{}` where `{want}` is expected in argument {} of `{callee}`",
                                        a.ty,
                                        i + 1
                                    ),
                                )
                                .at(a.span, "here"),
                            );
                        }
                    }
                }
                expr.ty = sig.ret.clone();
            }
            ExprKind::Index(base, index) => {
                self.check_expr(base);
                self.check_expr(index);
                // Decay arrays.
                if let Type::Array(elem, _) = &base.ty {
                    base.ty = Type::ptr_to((**elem).clone());
                }
                if !index.ty.is_integer() {
                    self.diags.push(
                        Diagnostic::error("E0329", "array index must be an integer")
                            .at(index.span, "here"),
                    );
                }
                match base.ty.pointee() {
                    Some(elem) => expr.ty = elem.clone(),
                    None => {
                        self.diags.push(
                            Diagnostic::error(
                                "E0330",
                                format!("cannot index value of type `{}`", base.ty),
                            )
                            .at(base.span, "here"),
                        );
                        expr.ty = Type::int();
                    }
                }
            }
            ExprKind::Member { base, name, arrow } => {
                self.check_expr(base);
                if let Type::Array(elem, _) = &base.ty {
                    base.ty = Type::ptr_to((**elem).clone());
                }
                let struct_ty = if *arrow {
                    match &base.ty {
                        Type::Pointer(inner) => Some((**inner).clone()),
                        _ => None,
                    }
                } else {
                    Some(base.ty.clone())
                };
                let Some(Type::Struct(sname)) = struct_ty else {
                    self.diags.push(
                        Diagnostic::error(
                            "E0331",
                            if *arrow {
                                format!("`->` requires a pointer to struct, found `{}`", base.ty)
                            } else {
                                format!("`.` requires a struct, found `{}`", base.ty)
                            },
                        )
                        .at(base.span, "here"),
                    );
                    expr.ty = Type::int();
                    return;
                };
                let Some(def) = self.structs.get(&sname) else {
                    self.diags.push(
                        Diagnostic::error("E0309", format!("unknown struct `{sname}`"))
                            .at(base.span, "here"),
                    );
                    expr.ty = Type::int();
                    return;
                };
                match def.field(name) {
                    Some((_, fty)) => expr.ty = fty.clone(),
                    None => {
                        self.diags.push(
                            Diagnostic::error(
                                "E0332",
                                format!("`struct {sname}` has no field `{name}`"),
                            )
                            .at(expr.span, "here"),
                        );
                        expr.ty = Type::int();
                    }
                }
            }
            ExprKind::Unary(op, inner) => {
                self.check_expr(inner);
                if let Type::Array(elem, _) = &inner.ty {
                    inner.ty = Type::ptr_to((**elem).clone());
                }
                match op {
                    UnOp::Neg | UnOp::BitNot => {
                        if !inner.ty.is_integer() {
                            self.diags.push(
                                Diagnostic::error(
                                    "E0333",
                                    format!("unary `{}` requires an integer operand", op_str(*op)),
                                )
                                .at(inner.span, "here"),
                            );
                        }
                        expr.ty = inner.ty.clone();
                    }
                    UnOp::Not => {
                        require_scalar(self, inner, "operand of `!`");
                        expr.ty = Type::int();
                    }
                    UnOp::Deref => match inner.ty.pointee() {
                        Some(elem) => expr.ty = elem.clone(),
                        None => {
                            self.diags.push(
                                Diagnostic::error(
                                    "E0334",
                                    format!("cannot dereference `{}`", inner.ty),
                                )
                                .at(inner.span, "here"),
                            );
                            expr.ty = Type::int();
                        }
                    },
                    UnOp::AddrOf => {
                        if !is_lvalue(&inner.kind) {
                            self.diags.push(
                                Diagnostic::error(
                                    "E0335",
                                    "cannot take the address of this expression",
                                )
                                .at(inner.span, "here"),
                            );
                        }
                        expr.ty = Type::ptr_to(inner.ty.clone());
                    }
                }
            }
            ExprKind::Postfix { op, expr: inner } | ExprKind::PrefixIncDec { op, expr: inner } => {
                self.check_expr(inner);
                if !inner.ty.is_integer() && !inner.ty.is_pointer() {
                    self.diags.push(
                        Diagnostic::error(
                            "E0336",
                            format!("`{}` requires an integer or pointer", op_str_incdec(*op)),
                        )
                        .at(inner.span, "here"),
                    );
                }
                if !is_lvalue(&inner.kind) {
                    self.diags.push(
                        Diagnostic::error(
                            "E0337",
                            format!("target of `{}` must be assignable", op_str_incdec(*op)),
                        )
                        .at(inner.span, "here"),
                    );
                }
                expr.ty = inner.ty.clone();
            }
            ExprKind::Binary(op, lhs, rhs) => {
                self.check_expr(lhs);
                self.check_expr(rhs);
                decay_array(lhs);
                decay_array(rhs);
                expr.ty = self.check_binary(*op, lhs, rhs, expr.span);
            }
            ExprKind::Assign(op, lhs, rhs) => {
                self.check_expr(lhs);
                self.check_expr(rhs);
                decay_array(rhs);
                if !is_lvalue(&lhs.kind) {
                    self.diags.push(
                        Diagnostic::error("E0338", "left side of assignment is not assignable")
                            .at(lhs.span, "here"),
                    );
                }
                if let AssignOp::Compound(bop) = op {
                    expr.ty = self.check_binary(*bop, lhs, rhs, expr.span);
                } else {
                    if lhs.ty.is_integer() && rhs.ty.is_pointer() {
                        self.diags.push(
                            Diagnostic::warning(
                                "W0339",
                                format!(
                                    "assigning `{}` to `{}` discards the pointer",
                                    rhs.ty, lhs.ty
                                ),
                            )
                            .at(expr.span, "here"),
                        );
                    }
                    expr.ty = lhs.ty.clone();
                }
            }
            ExprKind::Ternary(cond, then, els) => {
                self.check_expr(cond);
                require_scalar(self, cond, "?: condition");
                self.check_expr(then);
                self.check_expr(els);
                decay_array(then);
                decay_array(els);
                expr.ty = if then.ty == els.ty {
                    then.ty.clone()
                } else if then.ty.is_integer() && els.ty.is_integer() {
                    usual_arith(&then.ty, &els.ty)
                } else if then.ty.is_pointer() || els.ty.is_pointer() {
                    if then.ty.is_pointer() {
                        then.ty.clone()
                    } else {
                        els.ty.clone()
                    }
                } else {
                    Type::int()
                };
            }
            ExprKind::Cast(ty, inner) => {
                self.check_expr(inner);
                decay_array(inner);
                if matches!(ty, Type::Struct(_)) {
                    self.diags.push(
                        Diagnostic::error("E0340", "cannot cast to a struct type")
                            .at(expr.span, "here"),
                    );
                }
                if matches!(inner.ty, Type::Struct(_)) {
                    self.diags.push(
                        Diagnostic::error("E0341", "cannot cast a struct value")
                            .at(inner.span, "here"),
                    );
                }
                expr.ty = ty.clone();
            }
            ExprKind::SizeofType(ty) => {
                let size = self.type_size(ty);
                *expr = Expr::new(
                    ExprKind::IntLit(size as i128, polyglot_syntax::lex::Suffix::Long),
                    expr.span,
                );
                // Note: replaced; ty set below by caller pattern? Set here.
                expr.ty = Type::long();
            }
            ExprKind::SizeofExpr(inner) => {
                self.check_expr(inner);
                // sizeof does NOT decay arrays: use the declared type.
                let size = self.type_size(&inner.ty);
                *expr = Expr::new(
                    ExprKind::IntLit(size as i128, polyglot_syntax::lex::Suffix::Long),
                    expr.span,
                );
                expr.ty = Type::long();
            }
            ExprKind::Comma(lhs, rhs) => {
                self.check_expr(lhs);
                self.check_expr(rhs);
                decay_array(rhs);
                expr.ty = rhs.ty.clone();
            }
            ExprKind::InitList(_) => {
                // eprintln!("[sema] check_expr InitList");
                self.diags.push(
                    Diagnostic::error("E0342", "initializer list is not valid in this context")
                        .at(expr.span, "here"),
                );
                expr.ty = Type::int();
            }
        }
    }

    fn check_binary(&mut self, op: BinOp, lhs: &mut Expr, rhs: &mut Expr, span: Span) -> Type {
        let lt = lhs.ty.clone();
        let rt = rhs.ty.clone();

        if op.is_comparison() || matches!(op, BinOp::LAnd | BinOp::LOr) {
            if matches!(op, BinOp::LAnd | BinOp::LOr) {
                require_scalar(self, lhs, "operand");
                require_scalar(self, rhs, "operand");
            } else {
                // Comparisons need two scalars of related kinds.
                if lt.is_pointer() && rt.is_integer() && !rt.is_pointer() {
                    // ok: ptr vs null-ish int
                } else if rt.is_pointer() && lt.is_integer() {
                    // ok
                } else if lt.is_pointer() != rt.is_pointer() {
                    self.diags.push(
                        Diagnostic::warning(
                            "W0343",
                            format!("comparison between `{lt}` and `{rt}`"),
                        )
                        .at(span, "here"),
                    );
                }
            }
            return Type::int();
        }

        match op {
            BinOp::Shl | BinOp::Shr => {
                if !lt.is_integer() || !rt.is_integer() {
                    self.diags.push(
                        Diagnostic::error("E0344", "shift operands must be integers")
                            .at(span, "here"),
                    );
                    return Type::int();
                }
                lt.clone()
            }
            BinOp::Add => {
                if lt.is_pointer() && rt.is_integer() {
                    return lt.clone();
                }
                if rt.is_pointer() && lt.is_integer() {
                    return rt.clone();
                }
                if lt.is_pointer() && rt.is_pointer() {
                    self.diags.push(
                        Diagnostic::error("E0345", "cannot add two pointers").at(span, "here"),
                    );
                    return Type::long();
                }
                if !lt.is_integer() || !rt.is_integer() {
                    self.diags.push(
                        Diagnostic::error(
                            "E0346",
                            format!("invalid operands to `+` (`{lt}` and `{rt}`)"),
                        )
                        .at(span, "here"),
                    );
                    return Type::int();
                }
                usual_arith(&lt, &rt)
            }
            BinOp::Sub => {
                if lt.is_pointer() && rt.is_integer() {
                    return lt.clone();
                }
                if lt.is_pointer() && rt.is_pointer() {
                    return Type::long(); // ptrdiff
                }
                if !lt.is_integer() || !rt.is_integer() {
                    self.diags.push(
                        Diagnostic::error(
                            "E0346",
                            format!("invalid operands to `-` (`{lt}` and `{rt}`)"),
                        )
                        .at(span, "here"),
                    );
                    return Type::int();
                }
                usual_arith(&lt, &rt)
            }
            BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::BitAnd | BinOp::BitXor | BinOp::BitOr => {
                if !lt.is_integer() || !rt.is_integer() {
                    let sym = op.symbol();
                    self.diags.push(
                        Diagnostic::error(
                            "E0346",
                            format!("invalid operands to `{sym}` (`{lt}` and `{rt}`)"),
                        )
                        .at(span, "here"),
                    );
                    return Type::int();
                }
                usual_arith(&lt, &rt)
            }
            _ => unreachable!(),
        }
    }

    fn type_size(&self, ty: &Type) -> u64 {
        match ty {
            Type::Struct(name) => match self.structs.get(name) {
                Some(def) => def.size,
                None => {
                    // Reported elsewhere; use a stable fallback.
                    0
                }
            },
            Type::Array(elem, len) => len.unwrap_or(0) * self.type_size(elem),
            other => other.size(),
        }
    }
}

fn require_scalar(ctx: &mut FnCtx, expr: &Expr, what: &str) {
    if !expr.ty.is_scalar() && !matches!(expr.ty, Type::Struct(_)) {
        // structs are not valid conditions
        if !expr.ty.is_scalar() {
            ctx.diags.push(
                Diagnostic::error(
                    "E0347",
                    format!("{what} must be a scalar, found `{}`", expr.ty),
                )
                .at(expr.span, "here"),
            );
        }
    }
}

fn is_lvalue(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Var(_)
            | ExprKind::Unary(UnOp::Deref, _)
            | ExprKind::Index(..)
            | ExprKind::Member { .. }
    )
}

fn decay_array(expr: &mut Expr) {
    if let Type::Array(elem, _) = &expr.ty {
        expr.ty = Type::ptr_to((**elem).clone());
    }
}

fn usual_arith(a: &Type, b: &Type) -> Type {
    let kind = match (a, b) {
        (Type::Int(IntKind::Long), _) | (_, Type::Int(IntKind::Long)) => IntKind::Long,
        _ => IntKind::Int,
    };
    Type::Int(kind)
}

fn int_lit_type(v: i128, suffix: polyglot_syntax::lex::Suffix) -> Type {
    use polyglot_syntax::lex::Suffix;
    match suffix {
        Suffix::Long | Suffix::LongUnsigned => Type::long(),
        _ => {
            if v > i32::MAX as i128 {
                Type::long()
            } else {
                Type::int()
            }
        }
    }
}

/// Assignment/argument compatibility: permissive for integers, strict
/// for struct identity, pointers inter-convert with a warning at the
/// call sites handled elsewhere.
fn compatible(want: &Type, got: &Type) -> bool {
    if want == got {
        return true;
    }
    matches!(
        (want, got),
        (Type::Int(_), Type::Int(_))
            | (Type::Pointer(_), Type::Pointer(_))
            | (Type::Pointer(_), Type::Int(_))
            | (Type::Int(_), Type::Pointer(_))
            | (Type::Array(..), _)
            | (_, Type::Array(..))
    )
}

fn op_str(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
        UnOp::BitNot => "~",
        UnOp::Deref => "*",
        UnOp::AddrOf => "&",
    }
}

fn op_str_incdec(op: IncDec) -> &'static str {
    match op {
        IncDec::Inc | IncDec::PostInc => "++",
        IncDec::Dec | IncDec::PostDec => "--",
    }
}

/// Evaluate a constant expression (used by case labels and array
/// lengths). Returns None for non-constant expressions.
pub fn const_eval(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::IntLit(v, _) => Some(*v),
        ExprKind::Unary(UnOp::Neg, inner) => const_eval(inner).map(|v| -v),
        ExprKind::Unary(UnOp::BitNot, inner) => const_eval(inner).map(|v| !v),
        ExprKind::Unary(UnOp::Not, inner) => const_eval(inner).map(|v| (v == 0) as i128),
        ExprKind::Binary(op, l, r) => {
            let (l, r) = (const_eval(l)?, const_eval(r)?);
            Some(match op {
                BinOp::Add => l.wrapping_add(r),
                BinOp::Sub => l.wrapping_sub(r),
                BinOp::Mul => l.wrapping_mul(r),
                BinOp::Div => {
                    if r == 0 {
                        return None;
                    }
                    l / r
                }
                BinOp::Mod => {
                    if r == 0 {
                        return None;
                    }
                    l % r
                }
                BinOp::Shl => l.wrapping_shl(r as u32),
                BinOp::Shr => l.wrapping_shr(r as u32),
                BinOp::BitAnd => l & r,
                BinOp::BitXor => l ^ r,
                BinOp::BitOr => l | r,
                BinOp::Eq => (l == r) as i128,
                BinOp::Ne => (l != r) as i128,
                BinOp::Lt => (l < r) as i128,
                BinOp::Gt => (l > r) as i128,
                BinOp::Le => (l <= r) as i128,
                BinOp::Ge => (l >= r) as i128,
                BinOp::LAnd => ((l != 0) && (r != 0)) as i128,
                BinOp::LOr => ((l != 0) || (r != 0)) as i128,
            })
        }
        _ => None,
    }
}
