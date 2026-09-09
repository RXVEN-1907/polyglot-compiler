//! MLIR-specific types

/// Optimization level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    None,
    Less,
    Default,
    Aggressive,
    Size,
    SizeAggressive,
}

impl Default for OptLevel {
    fn default() -> Self {
        OptLevel::Default
    }
}