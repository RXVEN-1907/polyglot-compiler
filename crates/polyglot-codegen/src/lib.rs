//! Polyglot Codegen - MLIR to Cranelift to machine code

mod cranelift_backend;
mod mlir_to_clif;

pub use cranelift_backend::Codegen;