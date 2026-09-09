//! Abstract syntax tree produced by the parser.
//!
//! Expression nodes carry a `ty` field that the type checker fills in;
//! after `polyglot-sema` runs, every expression is annotated.

use polyglot_core::span::Span;
use polyglot_core::{IntKind, StructDef, Type};

/// A complete translation unit.
#[derive(Debug, Default)]
pub struct TranslationUnit {
    pub structs: Vec<StructDef>,
    pub typedefs: Vec<(String, Type)>,
    pub globals: Vec<GlobalDef>,
    pub funcs: Vec<FunctionDef>,
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    BitAnd,
    BitXor,
    BitOr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    LAnd,
    LOr,
}

impl BinOp {
    /// Operator symbol, used in IR printing and tests.
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::BitAnd => "&",
            BinOp::BitXor => "^",
            BinOp::BitOr => "|",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::LAnd => "&&",
            BinOp::LOr => "||",
        }
    }

    pub fn is_comparison(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge)
    }
}

/// Unary prefix operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
    Deref,
    AddrOf,
}

/// `++` / `--`, applied before or after the operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncDec {
    Inc,
    Dec,
    PostInc,
    PostDec,
}

/// Assignment operators; `Simple` is plain `=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Simple,
    Compound(BinOp),
}

/// Initializer: either an expression or a braced list.
#[derive(Clone, Debug)]
pub enum Init {
    Expr(Expr),
    List(Vec<Init>),
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// Filled by the type checker; `Void` before checking.
    pub ty: Type,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span, ty: Type::Void }
    }
}

/// Human-readable tag used in lowering error messages.
impl ExprKind {
    pub fn kind_desc(&self) -> &'static str {
        match self {
            ExprKind::IntLit(..) => "integer literal",
            ExprKind::StrLit(_) => "string literal",
            ExprKind::Var(_) => "variable",
            ExprKind::Call { .. } => "function call",
            ExprKind::Index(..) => "index expression",
            ExprKind::Member { .. } => "member access",
            ExprKind::Unary(..) => "unary expression",
            ExprKind::Postfix { .. } | ExprKind::PrefixIncDec { .. } => "increment/decrement",
            ExprKind::Binary(..) => "binary expression",
            ExprKind::Assign(..) => "assignment",
            ExprKind::Ternary(..) => "conditional expression",
            ExprKind::Cast(..) => "cast",
            ExprKind::SizeofType(_) | ExprKind::SizeofExpr(_) => "sizeof",
            ExprKind::Comma(..) => "comma expression",
            ExprKind::InitList(_) => "initializer list",
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    /// Integer literal with the suffix it carried in source.
    IntLit(i128, crate::lex::Suffix),
    StrLit(String),
    Var(String),
    Call { callee: String, args: Vec<Expr> },
    Index(Box<Expr>, Box<Expr>),
    /// Field access; `arrow` selects `.` vs `->`.
    Member { base: Box<Expr>, name: String, arrow: bool },
    Unary(UnOp, Box<Expr>),
    Postfix { op: IncDec, expr: Box<Expr> },
    PrefixIncDec { op: IncDec, expr: Box<Expr> },
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign(AssignOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Cast(Type, Box<Expr>),
    SizeofType(Type),
    SizeofExpr(Box<Expr>),
    Comma(Box<Expr>, Box<Expr>),
    InitList(Vec<Init>),
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Expr(Expr),
    /// Declaration with optional initializer.
    Decl { ty: Type, name: String, array_len: Option<Expr>, init: Option<Init> },
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    /// Sections preserve source order; each label is `None` for
    /// `default:` or an integer case expression. Fallthrough between
    /// sections is preserved by lowering.
    Switch { scrutinee: Expr, sections: Vec<SwitchSection> },
    Break,
    Continue,
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Empty,
}

#[derive(Clone, Debug)]
pub struct SwitchSection {
    pub labels: Vec<Option<Expr>>,
    pub stmts: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// A function definition (`body = None`) is a declaration/prototype.
#[derive(Clone, Debug)]
pub struct FunctionDef {
    pub ret: Type,
    pub name: String,
    pub params: Vec<Param>,
    pub variadic: bool,
    pub body: Option<Vec<Stmt>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct GlobalDef {
    pub ty: Type,
    pub name: String,
    pub array_len: Option<u64>,
    pub init: Option<Init>,
    pub span: Span,
}

/// Convenience alias used by the parser for base-type parsing results.
pub type TypeRef = Type;

/// Helper used by sema to know the default int kinds table.
pub fn default_int_kinds() -> [IntKind; 3] {
    [IntKind::Char, IntKind::Int, IntKind::Long]
}
