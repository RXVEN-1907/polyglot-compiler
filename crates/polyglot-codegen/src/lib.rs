//! Polyglot Codegen - polyglot-ir to LLVM IR to machine code
//!
//! The active path for native code generation is `llvm_emit`: it turns
//! a polyglot-ir [`polyglot_ir::Module`] into LLVM 17 textual IR and
//! compiles that to a real object file with `llc -filetype=obj`.

mod llvm_emit;

pub use llvm_emit::{emit_ll, emit_ll_named, emit_object, find_llc};
