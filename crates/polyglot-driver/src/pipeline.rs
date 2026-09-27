//! The self-contained Polyglot pipeline for C sources:
//!
//! `parse_source` (syntax) -> `check` (sema) -> `lower_program` (IR)
//! -> `optimize_module` -> { `print_module`, LLVM object emission }.
//!
//! The frontend runs entirely in-process; no external compiler compiles
//! C source. Textual artifacts are Polyglot IR; `--backend native`
//! emits object code with `llc` and links with the system `cc`.

use anyhow::{Context, Result};
use polyglot_core::{DiagnosticEngine, SourceFile};
use polyglot_ir::print::print_module;
use polyglot_ir::{OptLevel as IrOptLevel, OptStats};
use std::path::{Path, PathBuf};

/// Full result of the front-end -> IR pipeline.
pub struct Compiled {
    /// Source file (with line map) diagnostics were rendered against.
    pub source: SourceFile,
    /// Diagnostics produced along the way (may contain warnings).
    pub diags: DiagnosticEngine,
    /// Number of functions in the lowered module.
    pub funcs: usize,
    /// Number of global variables in the lowered module.
    pub globals: usize,
    /// Optimization statistics.
    pub stats: OptStats,
    /// Textual IR, ready to write out.
    pub ir: String,
    /// The optimized module itself, for backends that consume IR
    /// programmatically (LLVM emission, native object code).
    pub module: polyglot_ir::Module,
}

/// Map the CLI optimization level onto the IR pass pipeline.
pub fn map_opt_level(level: crate::cli::OptLevelArg) -> IrOptLevel {
    use crate::cli::OptLevelArg as Arg;
    match level {
        Arg::None => IrOptLevel::None,
        Arg::Less => IrOptLevel::Less,
        Arg::Default => IrOptLevel::Default,
        Arg::Aggressive => IrOptLevel::Aggressive,
        // Size-oriented levels reuse the closest available pass depth;
        // final size tuning happens downstream at machine-code emission.
        Arg::Size => IrOptLevel::Default,
        Arg::SizeAggressive => IrOptLevel::Aggressive,
    }
}

/// Preprocess, parse and type-check one translation unit.
///
/// Rendered diagnostics are collected in `diags`; the caller decides how
/// to surface them. Returns `Err` only when the pipeline cannot continue.
pub fn parse_and_check(
    path: &Path,
    include_dirs: &[PathBuf],
    diags: &mut DiagnosticEngine,
) -> Result<(SourceFile, polyglot_sema::CheckedProgram)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read source file {}", path.display()))?;

    let (source, unit) = match polyglot_syntax::parse_source(path, &text, include_dirs, diags) {
        Ok(ok) => ok,
        Err(e) => fail(path, diags, &None, e),
    };
    if diags.has_errors() {
        fail(
            path,
            diags,
            &Some(source.clone()),
            anyhow::anyhow!("parsing failed"),
        );
    }

    let checked = match polyglot_sema::check(unit, diags) {
        Ok(ok) => ok,
        Err(e) => fail(path, diags, &Some(source.clone()), e),
    };
    if diags.has_errors() {
        fail(
            path,
            diags,
            &Some(source.clone()),
            anyhow::anyhow!("type checking failed"),
        );
    }

    Ok((source, checked))
}

/// Render all collected diagnostics (plus the hard failure, if any) and
/// exit the process with status 1.
fn fail(
    path: &Path,
    diags: &DiagnosticEngine,
    source: &Option<SourceFile>,
    err: anyhow::Error,
) -> ! {
    let mut out = String::new();
    if let Some(source) = source {
        diags.render(source, &mut out);
        if !out.trim().is_empty() {
            eprintln!("{}", out.trim_end());
        }
    }
    eprintln!("error: while compiling {}: {}", path.display(), err);
    std::process::exit(1);
}

/// Run the full pipeline to textual IR.
pub fn compile_to_ir(path: &Path, include_dirs: &[PathBuf], level: IrOptLevel) -> Result<Compiled> {
    let mut diags = DiagnosticEngine::new();
    let (source, checked) = parse_and_check(path, include_dirs, &mut diags)?;

    let mut module = polyglot_ir::lower_program(checked);
    let funcs = module.funcs.len();
    let globals = module.globals.len();
    let stats = polyglot_ir::optimize_module(&mut module, level);
    let ir = print_module(&module);

    Ok(Compiled {
        source,
        diags,
        funcs,
        globals,
        stats,
        ir,
        module,
    })
}

/// Type-check only; returns the diagnostics for the caller to render.
pub fn check_only(
    path: &Path,
    include_dirs: &[PathBuf],
) -> Result<(SourceFile, DiagnosticEngine, usize)> {
    let mut diags = DiagnosticEngine::new();
    let (source, checked) = parse_and_check(path, include_dirs, &mut diags)?;
    let nfuncs = checked.funcs.len();
    Ok((source, diags, nfuncs))
}

/// Render any non-error diagnostics (warnings) to stderr.
pub fn emit_warnings(diags: &DiagnosticEngine, source: &SourceFile) {
    if diags.warning_count() > 0 {
        let mut out = String::new();
        diags.render(source, &mut out);
        eprintln!("{}", out.trim_end());
    }
}
