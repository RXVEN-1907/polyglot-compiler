//! MLIR-specific types

/// Optimization level
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OptLevel {
    None,
    Less,
    #[default]
    Default,
    Aggressive,
    Size,
    SizeAggressive,
}
