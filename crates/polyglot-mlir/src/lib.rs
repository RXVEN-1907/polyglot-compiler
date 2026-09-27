//! Polyglot MLIR - MLIR integration for Polyglot compiler

pub mod context;
pub mod types;

pub use context::{MlirContextWrapper as MlirContext, MlirModuleWrapper as MlirModule};
