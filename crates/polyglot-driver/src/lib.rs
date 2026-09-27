//! Polyglot Driver - CLI, pipeline orchestration and project types.

pub mod cli;
pub mod pipeline;
pub mod simple_driver;
pub mod types;

pub use pipeline::{check_only, compile_to_ir, parse_and_check};
pub use simple_driver::run_simple_driver;
