//! Polyglot Codegen - MLIR to Cranelift to machine code
//!
//! The active path for native code generation is `llvm_emit`: it turns
//! a polyglot-ir [`polyglot_ir::Module`] into LLVM 17 textual IR and
//! compiles that to a real object file with `llc -filetype=obj`. The
//! MLIR/Cranelift modules are exploratory stubs kept for future work.

mod cranelift_backend;
mod llvm_emit;
mod mlir_to_clif;

pub use cranelift_backend::Codegen;
pub use llvm_emit::{emit_ll, emit_ll_named, emit_object, find_llc};
