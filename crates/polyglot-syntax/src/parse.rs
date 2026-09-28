//! Recursive-descent parser for the Polyglot C subset.

use crate::ast::*;
use crate::lex::{unexpected, Suffix, Token, TokenKind};
use polyglot_core::span::Span;
use polyglot_core::{Diagnostic, DiagnosticEngine, StructDef, StructField, Type};
use std::collections::HashMap;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Typedef name -> type, visible everywhere after declaration.
    typedef_map: std::collections::HashMap<String, Type>,
    /// Struct definitions parsed inside function bodies; drained into
    /// the translation unit by the top-level loop.
    pending_structs: Vec<StructDef>,
    /// Enum definitions parsed anywhere; drained alongside
    /// `pending_structs` (constants fold at parse time, so this only
    /// records the definitions for inspection).
    pending_enums: Vec<EnumDef>,
    /// Enumerator name -> value; constants fold to integer literals
    /// from their declaration point onward (parser-wide, like
    /// typedefs — block scoping is not modeled).
    consts: std::collections::HashMap<String, i128>,
    /// Set by the most recent `parse_base_type`: true when the last
    /// specifier word was `unsigned` (bit-fields are the only place
    /// the frontend records signedness).
    last_base_unsigned: bool,
    /// Set by the most recent `parse_base_type`: true when an enum
    /// body was just parsed, so a following `;` ends the declaration.
    last_was_enum_body: bool,
    /// Counter for unique anonymous struct/union tag names.
    anon_counter: usize,
}

/// Binary operator precedence; higher binds tighter.
fn binop_prec(op: BinOp) -> u8 {
    match op {
        BinOp::LOr => 1,
        BinOp::LAnd => 2,
        BinOp::BitOr => 3,
        BinOp::BitXor => 4,
        BinOp::BitAnd => 5,
        BinOp::Eq | BinOp::Ne => 6,
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 7,
        BinOp::Shl | BinOp::Shr => 8,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 10,
    }
}

fn punct_to_binop(p: &str) -> Option<BinOp> {
    Some(match p {
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "/" => BinOp::Div,
        "%" => BinOp::Mod,
        "<<" => BinOp::Shl,
        ">>" => BinOp::Shr,
        "&" => BinOp::BitAnd,
        "^" => BinOp::BitXor,
        "|" => BinOp::BitOr,
        "==" => BinOp::Eq,
        "!=" => BinOp::Ne,
        "<" => BinOp::Lt,
        ">" => BinOp::Gt,
        "<=" => BinOp::Le,
        ">=" => BinOp::Ge,
        "&&" => BinOp::LAnd,
        "||" => BinOp::LOr,
        _ => return None,
    })
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            typedef_map: std::collections::HashMap::new(),
            pending_structs: Vec::new(),
            pending_enums: Vec::new(),
            consts: std::collections::HashMap::new(),
            last_base_unsigned: false,
            last_was_enum_body: false,
            anon_counter: 0,
        }
    }

    // ---- token utilities -------------------------------------------------

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn next(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at_punct(&self, p: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Punct(q) if *q == p)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.at_punct(p) {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, p: &str, diags: &mut DiagnosticEngine) -> Span {
        if self.eat_punct(p) {
            return self.tokens[self.pos - 1].span;
        }
        let t = self.peek().clone();
        diags.push(unexpected(&t.kind, t.span, &format!("`{p}`")));
        t.span
    }

    fn expect_ident(&mut self, diags: &mut DiagnosticEngine) -> (String, Span) {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                let span = self.next().span;
                (name, span)
            }
            other => {
                let t = self.peek().clone();
                diags.push(unexpected(&other, t.span, "identifier"));
                (String::new(), t.span)
            }
        }
    }

    fn starts_base_type(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Ident(name) => {
                matches!(
                    name.as_str(),
                    "char"
                        | "short"
                        | "int"
                        | "long"
                        | "unsigned"
                        | "signed"
                        | "void"
                        | "struct"
                        | "union"
                        | "enum"
                        | "const"
                ) || self.typedef_map.contains_key(name)
            }
            _ => false,
        }
    }

    // ---- top level --------------------------------------------------------

    pub fn parse_translation_unit(
        &mut self,
        diags: &mut DiagnosticEngine,
    ) -> anyhow::Result<TranslationUnit> {
        let mut unit = TranslationUnit::default();
        while !matches!(self.peek_kind(), TokenKind::Eof) {
            if self.eat_punct(";") {
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "typedef") {
                self.next();
                self.parse_typedef(&mut unit, diags);
                continue;
            }
            self.parse_top_level_decl(&mut unit, diags);
            unit.structs.append(&mut self.pending_structs);
            unit.enums.append(&mut self.pending_enums);
        }
        Ok(unit)
    }

    fn parse_typedef(&mut self, unit: &mut TranslationUnit, diags: &mut DiagnosticEngine) {
        let base = self.parse_base_type(unit, diags);
        loop {
            let (name, _) = self.expect_ident(diags);
            let ty = self.declarator_type(base.clone(), diags);
            if !name.is_empty() {
                self.typedef_map.insert(name.clone(), ty.clone());
                unit.typedefs.push((name, ty));
            }
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(";", diags);
        unit.structs.append(&mut self.pending_structs);
        unit.enums.append(&mut self.pending_enums);
    }

    fn parse_top_level_decl(&mut self, unit: &mut TranslationUnit, diags: &mut DiagnosticEngine) {
        let start_span = self.peek().span;
        let base = self.parse_base_type(unit, diags);
        // Bare `struct Point { ... };` / `enum Color { ... };` definition.
        let ended_enum_body = self.last_was_enum_body;
        self.last_was_enum_body = false;
        if (matches!(base, Type::Struct(_)) || ended_enum_body) && self.at_punct(";") {
            self.next();
            return;
        }
        loop {
            let mut pointers = 0usize;
            while self.eat_punct("*") {
                pointers += 1;
            }
            let (name, name_span) = self.expect_ident(diags);
            let mut ty = base.clone();
            for _ in 0..pointers {
                ty = Type::ptr_to(ty);
            }
            // Array suffix at top level.
            let mut array_len: Option<Expr> = None;
            if self.eat_punct("[") {
                let len_expr = if self.at_punct("]") {
                    None
                } else {
                    Some(self.parse_conditional(diags))
                };
                self.expect_punct("]", diags);
                let len = len_expr.as_ref().and_then(const_array_len);
                ty = Type::Array(Box::new(ty), len);
                array_len = len_expr;
            }

            if self.at_punct("(") {
                // Function declaration or definition.
                let (params, variadic) = self.parse_params(unit, diags);
                let body = if self.at_punct("{") {
                    Some(self.parse_block(diags))
                } else {
                    self.expect_punct(";", diags);
                    None
                };
                if !name.is_empty() {
                    unit.funcs.push(FunctionDef {
                        ret: ty,
                        name,
                        params,
                        variadic,
                        body,
                        span: name_span,
                    });
                }
            } else {
                let init = self.parse_initializer(diags);
                self.expect_punct(";", diags);
                if !name.is_empty() {
                    unit.globals.push(GlobalDef {
                        ty,
                        name,
                        array_len: array_len.and_then(|e| const_array_len(&e)),
                        init,
                        span: start_span,
                    });
                }
            }
            if self.eat_punct(",") {
                continue;
            }
            break;
        }
    }

    fn parse_params(
        &mut self,
        unit: &mut TranslationUnit,
        diags: &mut DiagnosticEngine,
    ) -> (Vec<Param>, bool) {
        self.expect_punct("(", diags);
        let mut params = Vec::new();
        let mut variadic = false;
        if self.eat_punct(")") {
            return (params, variadic);
        }
        loop {
            if self.eat_punct(")") {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Punct("...")) {
                self.next();
                variadic = true;
                self.expect_punct(")", diags);
                break;
            }
            // `void` alone means an empty parameter list.
            if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "void")
                && matches!(self.peek_ahead_kind(1), TokenKind::Punct(")"))
            {
                self.next();
                self.next();
                break;
            }
            let base = self.parse_base_type(unit, diags);
            let mut pointers = 0usize;
            while self.eat_punct("*") {
                pointers += 1;
            }
            // Parameter name is optional in prototypes: `int f(int);`
            let (name, _) = match self.peek_kind().clone() {
                TokenKind::Ident(n)
                    if !matches!(
                        n.as_str(),
                        "char"
                            | "short"
                            | "int"
                            | "long"
                            | "unsigned"
                            | "signed"
                            | "void"
                            | "struct"
                    ) && !self.at_punct("[")
                        && !self.at_punct(",")
                        && !self.at_punct(")") =>
                {
                    let sp = self.next().span;
                    (n, sp)
                }
                _ => (String::new(), self.peek().span),
            };
            let mut ty = base;
            for _ in 0..pointers {
                ty = Type::ptr_to(ty);
            }
            // Array parameters decay to pointers.
            if self.eat_punct("[") {
                if !self.at_punct("]") {
                    self.parse_conditional(diags);
                }
                self.expect_punct("]", diags);
                ty = Type::ptr_to(match ty {
                    Type::Pointer(inner) => *inner,
                    other => other,
                });
            }
            params.push(Param { name, ty });
            if !self.eat_punct(",") {
                self.expect_punct(")", diags);
                break;
            }
        }
        (params, variadic)
    }

    fn peek_ahead_kind(&self, n: usize) -> &TokenKind {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    // ---- types -----------------------------------------------------------

    /// Parse a base type: specifiers plus optional `struct`/`union`/`enum`
    /// definition.
    fn parse_base_type(
        &mut self,
        unit: &mut TranslationUnit,
        diags: &mut DiagnosticEngine,
    ) -> Type {
        let mut has_long = false;
        let mut has_short = false;
        let mut has_int = false;
        let mut has_char = false;
        let mut has_sign = false;
        let mut is_void = false;
        let mut result: Option<Type> = None;
        self.last_base_unsigned = false;
        self.last_was_enum_body = false;

        while let TokenKind::Ident(word) = self.peek_kind().clone() {
            match word.as_str() {
                "unsigned" => {
                    self.next();
                    has_sign = true;
                    self.last_base_unsigned = true;
                }
                "signed" => {
                    self.next();
                    has_sign = true;
                }
                "const" => {
                    self.next();
                }
                "long" => {
                    self.next();
                    has_long = true;
                }
                "short" => {
                    self.next();
                    has_short = true;
                }
                "int" => {
                    self.next();
                    has_int = true;
                }
                "char" => {
                    self.next();
                    has_char = true;
                }
                "void" => {
                    self.next();
                    is_void = true;
                }
                "struct" => {
                    self.next();
                    result = Some(self.parse_struct_type(unit, diags, false));
                }
                "union" => {
                    self.next();
                    result = Some(self.parse_struct_type(unit, diags, true));
                }
                "enum" => {
                    self.next();
                    result = Some(self.parse_enum_type(diags));
                }
                _ => break,
            }
        }

        if let Some(ty) = result {
            return ty;
        }
        if is_void {
            return Type::Void;
        }
        if has_char {
            return Type::char_t();
        }
        if has_long {
            return Type::long();
        }
        if has_short || has_int || has_sign {
            // `unsigned`/`signed` without a sized specifier is `int`
            // (the frontend has no distinct unsigned types).
            return Type::int();
        }
        // Must be a typedef name.
        if let TokenKind::Ident(name) = self.peek_kind().clone() {
            if self.typedef_map.contains_key(&name) {
                let ty = self.typedef_map[&name].clone();
                self.next();
                return ty;
            }
        }
        diags.push(Diagnostic::error("E0203", "expected a type").at(self.peek().span, "here"));
        Type::int()
    }

    /// Parse `Name { fields... }` (definition or use) after `struct` or
    /// `union`; `is_union` selects the kind. Unions are represented as
    /// struct definitions with every member at offset 0.
    fn parse_struct_type(
        &mut self,
        unit: &mut TranslationUnit,
        diags: &mut DiagnosticEngine,
        is_union: bool,
    ) -> Type {
        // Anonymous definitions (`struct { int x; } p;`) get a unique
        // internal name (containing a `.`, which no identifier can) so
        // distinct anonymous definitions never collide in the table.
        let (name, _) = if self.at_punct("{") {
            self.anon_counter += 1;
            (format!(".anon{}", self.anon_counter), self.peek().span)
        } else {
            self.expect_ident(diags)
        };
        if self.at_punct("{") {
            self.next();
            let mut fields: Vec<StructField> = Vec::new();
            while !self.at_punct("}") && !matches!(self.peek_kind(), TokenKind::Eof) {
                let base = self.parse_base_type(unit, diags);
                let base_unsigned = self.last_base_unsigned;
                loop {
                    let mut pointers = 0usize;
                    while self.eat_punct("*") {
                        pointers += 1;
                    }
                    let mut fty = base.clone();
                    for _ in 0..pointers {
                        fty = Type::ptr_to(fty);
                    }
                    // An unnamed bit-field (`int :3;` padding or `int :0;`
                    // boundary) has no declarator at all.
                    if self.at_punct(":") {
                        self.next();
                        let width = self.parse_bitfield_width(&fty, false, diags);
                        if let Some(width) = width {
                            fields.push(StructField {
                                name: String::new(),
                                ty: fty,
                                offset: 0,
                                bitfield: Some(polyglot_core::BitfieldLayout {
                                    lsb: 0,
                                    width,
                                    signed: false,
                                }),
                            });
                        }
                        if !self.eat_punct(",") {
                            break;
                        }
                        continue;
                    }
                    let (fname, fspan) = self.expect_ident(diags);
                    // Array members.
                    if self.eat_punct("[") {
                        let len_expr = if self.at_punct("]") {
                            None
                        } else {
                            Some(self.parse_conditional(diags))
                        };
                        self.expect_punct("]", diags);
                        if let Some(len) = len_expr.and_then(|e| const_array_len(&e)) {
                            fty = Type::Array(Box::new(fty), Some(len));
                        } else {
                            diags.push(
                                Diagnostic::error(
                                    "E0210",
                                    "array member requires a constant length",
                                )
                                .at(fspan, "here"),
                            );
                            fty = Type::Array(Box::new(fty), Some(0));
                        }
                    }
                    // Bit-field width suffix.
                    let bitfield = if self.eat_punct(":") {
                        self.parse_bitfield_width(&fty, true, diags).map(|width| {
                            polyglot_core::BitfieldLayout {
                                lsb: 0,
                                width,
                                signed: !base_unsigned,
                            }
                        })
                    } else {
                        None
                    };
                    if !fname.is_empty() {
                        if fields.iter().any(|f| f.name == fname) {
                            diags.push(
                                Diagnostic::error("E0211", format!("duplicate field `{fname}`"))
                                    .at(fspan, "here"),
                            );
                        } else {
                            fields.push(StructField {
                                name: fname,
                                ty: fty,
                                offset: 0,
                                bitfield,
                            });
                        }
                    }
                    if !self.eat_punct(",") {
                        break;
                    }
                }
                self.expect_punct(";", diags);
            }
            self.expect_punct("}", diags);
            let mut def = StructDef {
                name: name.clone(),
                fields,
                size: 0,
                align: 0,
                is_union,
            };
            // Build a temporary struct table from already-parsed structs for nested struct layout.
            let mut temp_table: HashMap<String, StructDef> = HashMap::new();
            for s in &unit.structs {
                temp_table.insert(s.name.clone(), s.clone());
            }
            for s in &self.pending_structs {
                temp_table.insert(s.name.clone(), s.clone());
            }
            def.layout(&temp_table);
            if !unit.structs.iter().any(|s| s.name == name)
                && !self.pending_structs.iter().any(|s| s.name == name)
            {
                self.pending_structs.push(def);
            }
        }
        Type::Struct(name)
    }

    /// Parse the width of a bit-field after the `:`; diagnoses
    /// non-constant, negative, over-wide and (for named fields)
    /// zero-width widths. Returns `None` on a hard error (the member
    /// keeps no bit-field marker).
    fn parse_bitfield_width(
        &mut self,
        ty: &Type,
        named: bool,
        diags: &mut DiagnosticEngine,
    ) -> Option<u32> {
        let expr = self.parse_conditional(diags);
        let value = match const_int(&expr) {
            Some(v) => v,
            None => {
                diags.push(
                    Diagnostic::error(
                        "E0214",
                        "bit-field width must be a non-negative integer constant",
                    )
                    .at(expr.span, "here"),
                );
                return None;
            }
        };
        if value < 0 {
            diags.push(
                Diagnostic::error(
                    "E0214",
                    "bit-field width must be a non-negative integer constant",
                )
                .at(expr.span, "here"),
            );
            return None;
        }
        let width = value as u32;
        if u64::from(width) > ty.size() * 8 {
            diags.push(
                Diagnostic::error(
                    "E0216",
                    format!("bit-field width {width} exceeds the width of `{ty}`"),
                )
                .at(expr.span, "here"),
            );
            return None;
        }
        if width == 0 && named {
            diags.push(
                Diagnostic::error("E0215", "zero-width bit-fields must be unnamed")
                    .at(expr.span, "here"),
            );
        }
        Some(width)
    }

    /// Parse an `enum` definition or use: `enum Name { A, B = 3, C };`
    ///
    /// Constants fold to integer literals at parse time (the same
    /// model as `#define`); the type itself is the underlying `int`.
    fn parse_enum_type(&mut self, diags: &mut DiagnosticEngine) -> Type {
        // Anonymous enums (`enum { A, B } x;`) have no tag.
        let (tag, _) = if self.at_punct("{") {
            (String::new(), self.peek().span)
        } else {
            self.expect_ident(diags)
        };
        if self.at_punct("{") {
            self.next();
            let mut constants: Vec<(String, i128)> = Vec::new();
            let mut next_value: i128 = 0;
            while !self.at_punct("}") && !matches!(self.peek_kind(), TokenKind::Eof) {
                let (cname, cspan) = self.expect_ident(diags);
                let value = if self.eat_punct("=") {
                    let expr = self.parse_conditional(diags);
                    match const_int(&expr) {
                        Some(v) => v,
                        None => {
                            diags.push(
                                Diagnostic::error(
                                    "E0213",
                                    "enumerator value must be an integer constant",
                                )
                                .at(expr.span, "here"),
                            );
                            0
                        }
                    }
                } else {
                    next_value
                };
                next_value = value.wrapping_add(1);
                if !cname.is_empty() {
                    if self.consts.contains_key(&cname) {
                        diags.push(
                            Diagnostic::error(
                                "E0212",
                                format!("redefinition of enumerator `{cname}`"),
                            )
                            .at(cspan, "here"),
                        );
                    } else {
                        self.consts.insert(cname.clone(), value);
                    }
                    constants.push((cname, value));
                }
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct("}", diags);
            self.last_was_enum_body = true;
            self.pending_enums.push(EnumDef {
                name: tag,
                constants,
            });
        }
        // Enums behave as their underlying int throughout the frontend.
        Type::int()
    }

    /// Wrap `base` in pointer levels parsed before an identifier.
    fn declarator_type(&mut self, base: Type, diags: &mut DiagnosticEngine) -> Type {
        let mut ty = base;
        while self.eat_punct("*") {
            ty = Type::ptr_to(ty);
        }
        // Optional array suffix in typedefs.
        if self.eat_punct("[") {
            let len_expr = if self.at_punct("]") {
                None
            } else {
                Some(self.parse_conditional(diags))
            };
            self.expect_punct("]", diags);
            if let Some(len) = len_expr.and_then(|e| const_array_len(&e)) {
                ty = Type::Array(Box::new(ty), Some(len));
            }
        }
        ty
    }

    // ---- statements ------------------------------------------------------

    fn parse_block(&mut self, diags: &mut DiagnosticEngine) -> Vec<Stmt> {
        self.expect_punct("{", diags);
        let mut stmts = Vec::new();
        // Parse statements until we find the matching closing brace.
        // Track brace depth to handle nested braces in initializers.
        let mut brace_depth = 1;
        while brace_depth > 0 && !matches!(self.peek_kind(), TokenKind::Eof) {
            if self.at_punct("{") {
                brace_depth += 1;
            } else if self.at_punct("}") {
                brace_depth -= 1;
                if brace_depth == 0 {
                    break;
                }
            }
            let span = self.peek().span;
            if self.starts_base_type() {
                let mut decls = self.parse_declaration_stmts(diags);
                stmts.append(&mut decls);
                continue;
            }
            let stmt = self.parse_stmt(diags);
            let _ = span;
            stmts.push(stmt);
        }
        self.expect_punct("}", diags);
        stmts
    }

    fn parse_stmt(&mut self, diags: &mut DiagnosticEngine) -> Stmt {
        let span = self.peek().span;
        let kind = match self.peek_kind().clone() {
            TokenKind::Punct("{") => StmtKind::Block(self.parse_block(diags)),
            TokenKind::Punct(";") => {
                self.next();
                StmtKind::Empty
            }
            TokenKind::Ident(word) => match word.as_str() {
                "if" => {
                    self.next();
                    self.expect_punct("(", diags);
                    let cond = self.parse_expression(diags);
                    self.expect_punct(")", diags);
                    let then = Box::new(self.parse_stmt(diags));
                    let els = if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "else") {
                        self.next();
                        Some(Box::new(self.parse_stmt(diags)))
                    } else {
                        None
                    };
                    StmtKind::If(cond, then, els)
                }
                "while" => {
                    self.next();
                    self.expect_punct("(", diags);
                    let cond = self.parse_expression(diags);
                    self.expect_punct(")", diags);
                    let body = Box::new(self.parse_stmt(diags));
                    StmtKind::While(cond, body)
                }
                "do" => {
                    self.next();
                    let body = Box::new(self.parse_stmt(diags));
                    if !matches!(self.peek_kind(), TokenKind::Ident(s) if s == "while") {
                        diags.push(
                            Diagnostic::error("E0204", "expected `while` after do-body")
                                .at(self.peek().span, "here"),
                        );
                    } else {
                        self.next();
                    }
                    self.expect_punct("(", diags);
                    let cond = self.parse_expression(diags);
                    self.expect_punct(")", diags);
                    self.expect_punct(";", diags);
                    StmtKind::DoWhile(body, cond)
                }
                "for" => {
                    self.next();
                    self.expect_punct("(", diags);
                    let init = if self.at_punct(";") {
                        self.next();
                        None
                    } else if self.starts_base_type() {
                        let decls = self.parse_declaration_stmts(diags);
                        // `for (int i = 0, j = 1; ...)` — wrap extras.
                        let first = if decls.is_empty() {
                            Stmt {
                                kind: StmtKind::Empty,
                                span,
                            }
                        } else {
                            let mut it = decls.into_iter();
                            let first = it.next().unwrap();
                            let rest: Vec<Stmt> = it.collect();
                            if !rest.is_empty() {
                                // Prepend rest via a block is wrong scoping-wise;
                                // instead extend after the for by rejecting.
                                // For simplicity keep them in the init position
                                // as a block: rare and semantically scoped to
                                // the loop, which matches C.
                                return Stmt {
                                    kind: StmtKind::Block(vec![
                                        first,
                                        Stmt {
                                            kind: StmtKind::Block(rest),
                                            span,
                                        },
                                    ]),
                                    span,
                                };
                            }
                            first
                        };
                        Some(Box::new(first))
                    } else {
                        let e = self.parse_expression(diags);
                        self.expect_punct(";", diags);
                        Some(Box::new(Stmt {
                            kind: StmtKind::Expr(e),
                            span,
                        }))
                    };
                    let cond = if self.at_punct(";") {
                        None
                    } else {
                        Some(self.parse_expression(diags))
                    };
                    self.expect_punct(";", diags);
                    let step = if self.at_punct(")") {
                        None
                    } else {
                        Some(self.parse_expression(diags))
                    };
                    self.expect_punct(")", diags);
                    let body = Box::new(self.parse_stmt(diags));
                    StmtKind::For {
                        init,
                        cond,
                        step,
                        body,
                    }
                }
                "break" => {
                    self.next();
                    self.expect_punct(";", diags);
                    StmtKind::Break
                }
                "continue" => {
                    self.next();
                    self.expect_punct(";", diags);
                    StmtKind::Continue
                }
                "return" => {
                    self.next();
                    let value = if self.at_punct(";") {
                        None
                    } else {
                        Some(self.parse_expression(diags))
                    };
                    self.expect_punct(";", diags);
                    StmtKind::Return(value)
                }
                "switch" => {
                    self.next();
                    self.expect_punct("(", diags);
                    let scrutinee = self.parse_expression(diags);
                    self.expect_punct(")", diags);
                    self.expect_punct("{", diags);
                    let mut sections: Vec<SwitchSection> = Vec::new();
                    while !self.at_punct("}") && !matches!(self.peek_kind(), TokenKind::Eof) {
                        let mut labels = Vec::new();
                        loop {
                            if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "case") {
                                self.next();
                                let value = self.parse_conditional(diags);
                                self.expect_punct(":", diags);
                                labels.push(Some(value));
                            } else if matches!(self.peek_kind(), TokenKind::Ident(s) if s == "default")
                            {
                                self.next();
                                self.expect_punct(":", diags);
                                labels.push(None);
                            } else {
                                break;
                            }
                        }
                        if labels.is_empty() {
                            let t = self.peek().clone();
                            diags.push(unexpected(&t.kind, t.span, "`case`, `default` or `}`"));
                            break;
                        }
                        let mut stmts = Vec::new();
                        while !self.at_punct("}")
                            && !matches!(self.peek_kind(), TokenKind::Ident(s) if s == "case" || s == "default")
                            && !matches!(self.peek_kind(), TokenKind::Eof)
                        {
                            stmts.push(self.parse_stmt(diags));
                        }
                        sections.push(SwitchSection { labels, stmts });
                    }
                    self.expect_punct("}", diags);
                    StmtKind::Switch {
                        scrutinee,
                        sections,
                    }
                }
                _ => {
                    if self.starts_base_type() {
                        let mut decls = self.parse_declaration_stmts(diags);
                        if decls.len() == 1 {
                            return decls.pop().unwrap();
                        }
                        return Stmt {
                            kind: StmtKind::Block(decls),
                            span,
                        };
                    }
                    let e = self.parse_expression(diags);
                    self.expect_punct(";", diags);
                    StmtKind::Expr(e)
                }
            },
            _ => {
                if self.starts_base_type() {
                    let mut decls = self.parse_declaration_stmts(diags);
                    if decls.len() == 1 {
                        return decls.pop().unwrap();
                    }
                    return Stmt {
                        kind: StmtKind::Block(decls),
                        span,
                    };
                }
                let e = self.parse_expression(diags);
                self.expect_punct(";", diags);
                StmtKind::Expr(e)
            }
        };
        Stmt { kind, span }
    }

    fn parse_declaration_stmts(&mut self, diags: &mut DiagnosticEngine) -> Vec<Stmt> {
        let span = self.peek().span;
        // A throwaway unit collects any inline struct definitions so they
        // are not lost; the sema stage receives them via the statement's
        // declared type name.
        let mut scratch = TranslationUnit::default();
        // Re-use the main typedef table through `self`.
        let base = self.parse_base_type_with(&mut scratch, diags);
        let ended_enum_body = self.last_was_enum_body;
        self.last_was_enum_body = false;
        // A bare `struct P { ... };` / `union U { ... };` /
        // `enum E { ... };` definition inside a block declares no
        // variables.
        if (matches!(base, Type::Struct(_)) || ended_enum_body) && self.eat_punct(";") {
            return Vec::new();
        }
        let mut decls: Vec<Stmt> = Vec::new();
        loop {
            let mut pointers = 0usize;
            while self.eat_punct("*") {
                pointers += 1;
            }
            let (name, _) = self.expect_ident(diags);
            let mut ty = base.clone();
            for _ in 0..pointers {
                ty = Type::ptr_to(ty);
            }
            let mut array_len: Option<Expr> = None;
            if self.eat_punct("[") {
                let len_expr = if self.at_punct("]") {
                    None
                } else {
                    Some(self.parse_conditional(diags))
                };
                self.expect_punct("]", diags);
                let len = len_expr.as_ref().and_then(const_array_len);
                ty = Type::Array(Box::new(ty), len);
                array_len = len_expr;
            }
            let init = self.parse_initializer(diags);
            decls.push(Stmt {
                kind: StmtKind::Decl {
                    ty,
                    name,
                    array_len,
                    init,
                },
                span,
            });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(";", diags);
        decls
    }

    /// Like `parse_base_type` but struct definitions found inside a
    /// local declaration are recorded on `scratch` and merged into the
    /// enclosing unit by the caller through `parse_translation_unit`'s
    /// struct table (local structs are rare; we support named reuse).
    fn parse_base_type_with(
        &mut self,
        unit: &mut TranslationUnit,
        diags: &mut DiagnosticEngine,
    ) -> Type {
        self.parse_base_type(unit, diags)
    }

    // ---- expressions -----------------------------------------------------

    pub fn parse_expression(&mut self, diags: &mut DiagnosticEngine) -> Expr {
        let first = self.parse_assignment_expr(diags);
        if !self.at_punct(",") {
            return first;
        }
        let mut cur = first;
        while self.eat_punct(",") {
            let rhs = self.parse_assignment_expr(diags);
            let span = cur.span.merge(rhs.span);
            cur = Expr::new(ExprKind::Comma(Box::new(cur), Box::new(rhs)), span);
        }
        cur
    }

    fn parse_assignment_expr(&mut self, diags: &mut DiagnosticEngine) -> Expr {
        let lhs = self.parse_conditional(diags);
        let op = match self.peek_kind() {
            TokenKind::Punct("=") => Some(AssignOp::Simple),
            TokenKind::Punct("+=") => Some(AssignOp::Compound(BinOp::Add)),
            TokenKind::Punct("-=") => Some(AssignOp::Compound(BinOp::Sub)),
            TokenKind::Punct("*=") => Some(AssignOp::Compound(BinOp::Mul)),
            TokenKind::Punct("/=") => Some(AssignOp::Compound(BinOp::Div)),
            TokenKind::Punct("%=") => Some(AssignOp::Compound(BinOp::Mod)),
            TokenKind::Punct("<<=") => Some(AssignOp::Compound(BinOp::Shl)),
            TokenKind::Punct(">>=") => Some(AssignOp::Compound(BinOp::Shr)),
            TokenKind::Punct("&=") => Some(AssignOp::Compound(BinOp::BitAnd)),
            TokenKind::Punct("|=") => Some(AssignOp::Compound(BinOp::BitOr)),
            TokenKind::Punct("^=") => Some(AssignOp::Compound(BinOp::BitXor)),
            _ => None,
        };
        if let Some(op) = op {
            self.next();
            let rhs = self.parse_assignment_expr(diags);
            let span = lhs.span.merge(rhs.span);
            return Expr::new(ExprKind::Assign(op, Box::new(lhs), Box::new(rhs)), span);
        }
        lhs
    }

    fn parse_conditional(&mut self, diags: &mut DiagnosticEngine) -> Expr {
        let cond = self.parse_binary(0, diags);
        if !self.at_punct("?") {
            return cond;
        }
        self.next();
        let then = self.parse_expression(diags);
        self.expect_punct(":", diags);
        let els = self.parse_conditional(diags);
        let span = cond.span.merge(els.span);
        Expr::new(
            ExprKind::Ternary(Box::new(cond), Box::new(then), Box::new(els)),
            span,
        )
    }

    fn parse_binary(&mut self, min_prec: u8, diags: &mut DiagnosticEngine) -> Expr {
        let mut lhs = self.parse_unary(diags);
        while let TokenKind::Punct(p) = self.peek_kind() {
            let op = match punct_to_binop(p) {
                Some(op) => op,
                None => break,
            };
            let prec = binop_prec(op);
            if prec < min_prec {
                break;
            }
            self.next();
            let rhs = self.parse_binary(prec + 1, diags);
            let span = lhs.span.merge(rhs.span);
            lhs = Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span);
        }
        lhs
    }

    fn parse_unary(&mut self, diags: &mut DiagnosticEngine) -> Expr {
        let span = self.peek().span;
        match self.peek_kind().clone() {
            TokenKind::Punct("-") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                // Fold negative literals immediately for nicer IR.
                if let ExprKind::IntLit(v, suffix) = e.kind {
                    return Expr::new(ExprKind::IntLit(-v, suffix), span);
                }
                Expr::new(ExprKind::Unary(UnOp::Neg, Box::new(e)), span)
            }
            TokenKind::Punct("+") => {
                self.next();
                self.parse_unary(diags)
            }
            TokenKind::Punct("!") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(ExprKind::Unary(UnOp::Not, Box::new(e)), span)
            }
            TokenKind::Punct("~") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(ExprKind::Unary(UnOp::BitNot, Box::new(e)), span)
            }
            TokenKind::Punct("*") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(ExprKind::Unary(UnOp::Deref, Box::new(e)), span)
            }
            TokenKind::Punct("&") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(ExprKind::Unary(UnOp::AddrOf, Box::new(e)), span)
            }
            TokenKind::Punct("++") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(
                    ExprKind::PrefixIncDec {
                        op: IncDec::Inc,
                        expr: Box::new(e),
                    },
                    span,
                )
            }
            TokenKind::Punct("--") => {
                self.next();
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(
                    ExprKind::PrefixIncDec {
                        op: IncDec::Dec,
                        expr: Box::new(e),
                    },
                    span,
                )
            }
            TokenKind::Ident(word) if word == "sizeof" => {
                self.next();
                if self.at_punct("(") {
                    // Could be sizeof(type) or sizeof(expr).
                    let save = self.pos;
                    self.next();
                    if let Some(ty) = self.try_parse_type_name(diags) {
                        if self.eat_punct(")") {
                            return Expr::new(ExprKind::SizeofType(ty), span);
                        }
                    }
                    self.pos = save;
                }
                let e = self.parse_unary(diags);
                let span = span.merge(e.span);
                Expr::new(ExprKind::SizeofExpr(Box::new(e)), span)
            }
            TokenKind::Punct("(") => {
                // Cast or parenthesized expression.
                let save = self.pos;
                self.next();
                if let Some(ty) = self.try_parse_type_name(diags) {
                    if self.eat_punct(")") {
                        let e = self.parse_unary(diags);
                        let span = span.merge(e.span);
                        return Expr::new(ExprKind::Cast(ty, Box::new(e)), span);
                    }
                }
                self.pos = save;
                self.next(); // consume '('
                let e = self.parse_expression(diags);
                self.expect_punct(")", diags);
                let span = span.merge(e.span);
                Expr { span, ..e }
            }
            _ => self.parse_postfix(diags),
        }
    }

    /// Try to parse a type name at the current position (used for
    /// casts and `sizeof`); restores nothing — caller saves position.
    fn try_parse_type_name(&mut self, diags: &mut DiagnosticEngine) -> Option<Type> {
        let mut scratch = TranslationUnit::default();
        if !self.starts_base_type() {
            return None;
        }
        let base = self.parse_base_type(&mut scratch, diags);
        let mut ty = base;
        while self.eat_punct("*") {
            ty = Type::ptr_to(ty);
        }
        Some(ty)
    }

    fn parse_postfix(&mut self, diags: &mut DiagnosticEngine) -> Expr {
        let mut expr = self.parse_primary(diags);
        loop {
            let span = self.peek().span;
            match self.peek_kind() {
                TokenKind::Punct("(") if matches!(expr.kind, ExprKind::Var(_)) => {
                    self.next();
                    let mut args = Vec::new();
                    if !self.at_punct(")") {
                        loop {
                            args.push(self.parse_assignment_expr(diags));
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                    }
                    self.expect_punct(")", diags);
                    let callee = match expr.kind {
                        ExprKind::Var(name) => name,
                        _ => unreachable!(),
                    };
                    let span = expr.span.merge(span);
                    expr = Expr::new(ExprKind::Call { callee, args }, span);
                }
                TokenKind::Punct("[") => {
                    self.next();
                    let index = self.parse_expression(diags);
                    self.expect_punct("]", diags);
                    let span = expr.span.merge(span);
                    expr = Expr::new(ExprKind::Index(Box::new(expr), Box::new(index)), span);
                }
                TokenKind::Punct(".") => {
                    self.next();
                    let (name, _) = self.expect_ident(diags);
                    let span = expr.span.merge(span);
                    expr = Expr::new(
                        ExprKind::Member {
                            base: Box::new(expr),
                            name,
                            arrow: false,
                        },
                        span,
                    );
                }
                TokenKind::Punct("->") => {
                    self.next();
                    let (name, _) = self.expect_ident(diags);
                    let span = expr.span.merge(span);
                    expr = Expr::new(
                        ExprKind::Member {
                            base: Box::new(expr),
                            name,
                            arrow: true,
                        },
                        span,
                    );
                }
                TokenKind::Punct("++") => {
                    self.next();
                    let span = expr.span.merge(span);
                    expr = Expr::new(
                        ExprKind::Postfix {
                            op: IncDec::PostInc,
                            expr: Box::new(expr),
                        },
                        span,
                    );
                }
                TokenKind::Punct("--") => {
                    self.next();
                    let span = expr.span.merge(span);
                    expr = Expr::new(
                        ExprKind::Postfix {
                            op: IncDec::PostDec,
                            expr: Box::new(expr),
                        },
                        span,
                    );
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_primary(&mut self, diags: &mut DiagnosticEngine) -> Expr {
        let span = self.peek().span;
        match self.peek_kind().clone() {
            TokenKind::Int(v, suffix) => {
                self.next();
                Expr::new(ExprKind::IntLit(v, suffix), span)
            }
            TokenKind::Char(c) => {
                self.next();
                Expr::new(ExprKind::IntLit(c as i128, Suffix::None), span)
            }
            TokenKind::Str(_) => {
                // Adjacent string literals concatenate.
                let mut s = String::new();
                while let TokenKind::Str(part) = self.peek_kind().clone() {
                    self.next();
                    s.push_str(&part);
                }
                Expr::new(ExprKind::StrLit(s), span)
            }
            TokenKind::Ident(name) => {
                self.next();
                // Enumerator constants fold to integer literals (same
                // model as object-like macros).
                if let Some(&v) = self.consts.get(&name) {
                    Expr::new(ExprKind::IntLit(v, Suffix::None), span)
                } else {
                    Expr::new(ExprKind::Var(name), span)
                }
            }
            other => {
                diags.push(unexpected(&other, span, "an expression"));
                // Error recovery: skip one token.
                self.next();
                Expr::new(ExprKind::IntLit(0, Suffix::None), span)
            }
        }
    }

    /// Parse an initializer after a declarator: `= expr` or `= { ... }`.
    fn parse_initializer(&mut self, diags: &mut DiagnosticEngine) -> Option<Init> {
        if !self.eat_punct("=") {
            return None;
        }
        self.parse_initializer_after_equals(diags)
    }

    /// Parse an initializer after the `=` has been consumed.
    /// Handles both expressions and brace-enclosed initializer lists.
    fn parse_initializer_after_equals(&mut self, diags: &mut DiagnosticEngine) -> Option<Init> {
        if self.at_punct("{") {
            self.next();
            let mut items = Vec::new();
            while !self.at_punct("}") && !matches!(self.peek_kind(), TokenKind::Eof) {
                // Recursive call for brace items: parse an expression (no `=` expected)
                items.push(self.parse_initializer_item(diags)?);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct("}", diags);
            return Some(Init::List(items));
        }
        Some(Init::Expr(self.parse_assignment_expr(diags)))
    }

    /// Parse a single initializer item (used for brace-enclosed lists).
    /// Does NOT expect a leading `=`.
    fn parse_initializer_item(&mut self, diags: &mut DiagnosticEngine) -> Option<Init> {
        if self.at_punct("{") {
            // Nested brace initializer
            self.next();
            let mut items = Vec::new();
            while !self.at_punct("}") && !matches!(self.peek_kind(), TokenKind::Eof) {
                items.push(self.parse_initializer_item(diags)?);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct("}", diags);
            return Some(Init::List(items));
        }
        Some(Init::Expr(self.parse_assignment_expr(diags)))
    }
}

/// Evaluate a constant array length expression (integer literals and
/// simple arithmetic on them).
fn const_array_len(expr: &Expr) -> Option<u64> {
    match &expr.kind {
        ExprKind::IntLit(v, _) if *v >= 0 => Some(*v as u64),
        _ => None,
    }
}

/// Evaluate a parser-level integer constant expression (enumerator
/// values, bit-field widths). Same semantics as `sema::const_eval`:
/// wrapping two's-complement arithmetic on `i128`.
fn const_int(expr: &Expr) -> Option<i128> {
    match &expr.kind {
        ExprKind::IntLit(v, _) => Some(*v),
        ExprKind::Unary(UnOp::Neg, inner) => const_int(inner).map(|v| v.wrapping_neg()),
        ExprKind::Unary(UnOp::BitNot, inner) => const_int(inner).map(|v| !v),
        ExprKind::Unary(UnOp::Not, inner) => const_int(inner).map(|v| (v == 0) as i128),
        ExprKind::Binary(op, l, r) => {
            let (l, r) = (const_int(l)?, const_int(r)?);
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
