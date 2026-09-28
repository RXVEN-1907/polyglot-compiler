//! Core types shared across every stage of the Polyglot compiler.
//!
//! This crate intentionally has zero dependencies: it defines the source
//! model ([`Span`], [`SourceFile`]), the diagnostic machinery
//! ([`Diagnostic`], [`DiagnosticEngine`]) and the C type model
//! ([`Type`]) used by the parser, type checker, IR and codegen.

pub mod diag;
pub mod span;
pub mod ty;

pub use diag::{Diagnostic, DiagnosticEngine, Label, Severity};
pub use span::{SourceFile, Span};
pub use ty::{BitfieldLayout, IntKind, StructDef, StructField, Type};
