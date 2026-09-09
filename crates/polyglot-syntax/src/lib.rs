//! Polyglot C front-end.
//!
//! This crate turns source text into an AST:
//!
//! 1. [`preprocess`] runs the C preprocessor subset (comments,
//!    `#include`, object-like and function-like `#define`,
//!    conditionals) and produces a single spliced text buffer plus a
//!    line map so diagnostics point at the file the user wrote.
//! 2. [`lex`] tokenizes the preprocessed text.
//! 3. [`parse`] builds the AST with recursive descent.

pub mod ast;
pub mod lex;
pub mod parse;
pub mod preprocess;

pub use ast::{Expr, ExprKind, FunctionDef, GlobalDef, Param, Stmt, TranslationUnit, TypeRef};
pub use lex::{Token, TokenKind};
pub use preprocess::preprocess;

use polyglot_core::span::SourceFile;

/// Run preprocessing + lexing for one translation unit.
///
/// `include_dirs` are searched after the including file's directory
/// for quoted includes and are the only place looked at for angle
/// includes.
pub fn parse_source(
    name: impl AsRef<std::path::Path>,
    text: &str,
    include_dirs: &[std::path::PathBuf],
    diags: &mut polyglot_core::DiagnosticEngine,
) -> anyhow::Result<(SourceFile, TranslationUnit)> {
    let pre = preprocess(name.as_ref(), text, include_dirs, diags)?;
    let source = SourceFile {
        name: name.as_ref().to_path_buf(),
        text: pre.text,
        line_map: pre.line_map,
        files: pre.files,
    };
    let tokens = lex::tokenize(&source.text);
    let unit = parse::Parser::new(tokens).parse_translation_unit(diags)?;
    Ok((source, unit))
}
