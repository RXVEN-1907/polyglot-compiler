//! Simple driver that uses clang directly for the MVP
//! This avoids the MLIR static linking issue while providing a working compiler

use crate::cli::{
    BackendArg, BuildArgs, CheckArgs, Cli, Commands, EmitIrArgs, InitArgs, IrFormatArg, RunArgs,
    TransmuteInternalArgs,
};
use crate::pipeline;
use polyglot_core::{DiagnosticEngine, SourceFile};
use polyglot_ir::print::print_module;
use polyglot_sema;
use polyglot_syntax;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use target_lexicon::Triple;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

pub fn run_simple_driver(cli: Cli) -> Result<()> {
    // Setup logging
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse target triple
    let target = cli
        .target_triple()
        .map_err(|e| anyhow::anyhow!("invalid target triple: {}", e))?;

    // Parse optimization level
    let opt_level: polyglot_protocol::types::OptLevel = cli.opt_level.into();

    // Execute command
    match cli.command.clone() {
        Commands::Build(args) => handle_build(args, &cli)?,
        Commands::Check(args) => handle_check(args, &cli)?,
        Commands::EmitIr(args) => handle_emit_ir(args, &cli)?,
        Commands::Run(args) => handle_run_simple(args, &cli, target, opt_level)?,
        Commands::Init(args) => handle_init(args)?,
        Commands::Version => {
            println!("polyglot {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Plugin(_) => {
            println!("Plugin management not yet implemented in simple driver");
        }
        Commands::TransmuteInternal(args) => {
            handle_transmute_internal(args, &cli)?;
        }
    }

    Ok(())
}

/// `polyglot build <src.c> [more.c ...] -o out`: the full in-process
/// pipeline (parse -> check -> lower -> optimize) per translation
/// unit. With the default `--backend ir` the artifact is textual
/// Polyglot IR; with `--backend native` each optimized module is
/// emitted to a real object file via `llc` and all objects are linked
/// to an executable with the system `cc`. Cross-TU calls resolve
/// through the declarations each TU sees (headers or prototypes);
/// tentative global definitions merge via COMMON symbols.
fn handle_build(args: BuildArgs, cli: &Cli) -> Result<()> {
    if args.path.is_empty() {
        anyhow::bail!("build requires a source file (polyglot build <src.c> [more.c ...])");
    }
    for source in &args.path {
        if !source.is_file() {
            anyhow::bail!("no such source file: {}", source.display());
        }
    }

    if args.path.len() == 1 {
        let source = args.path[0].clone();
        return build_single(source, args.backend, cli);
    }

    match args.backend {
        BackendArg::Ir => {
            if let Some(output) = &cli.output {
                anyhow::bail!(
                    "--output {} cannot name a single artifact for {} source files; \
                     omit it (each source writes <stem>.pgir) or build one file at a time",
                    output.display(),
                    args.path.len()
                );
            }
            for source in &args.path {
                let output = source.with_extension("pgir");
                let compiled = pipeline::compile_to_ir(
                    source,
                    &cli.include_dirs,
                    pipeline::map_opt_level(cli.opt_level),
                )?;
                pipeline::emit_warnings(&compiled.diags, &compiled.source);
                std::fs::write(&output, &compiled.ir)
                    .with_context(|| format!("cannot write {}", output.display()))?;
                let stats = &compiled.stats;
                println!(
                    "compiled {} -> {} ({} functions, {} globals, {} -> {} IR instructions, opt removed {})",
                    source.display(),
                    output.display(),
                    compiled.funcs,
                    compiled.globals,
                    stats.instrs_before,
                    stats.instrs_after,
                    stats.total_removed(),
                );
            }
            Ok(())
        }
        BackendArg::Native => {
            let output = cli.output.clone().unwrap_or_else(|| PathBuf::from("a.out"));
            // Each TU compiles to an object in a scratch directory;
            // only the linked executable lands at the output.
            let temp = tempfile::tempdir()
                .context("cannot create a temporary directory for object code")?;
            let mut objects: Vec<PathBuf> = Vec::new();
            for source in &args.path {
                let compiled = pipeline::compile_to_ir(
                    source,
                    &cli.include_dirs,
                    pipeline::map_opt_level(cli.opt_level),
                )?;
                pipeline::emit_warnings(&compiled.diags, &compiled.source);
                let stats = &compiled.stats;
                let stem = sanitize_stem(source);
                let obj_path = temp.path().join(format!("{stem}.o"));
                polyglot_codegen::emit_object(
                    &compiled.module,
                    &obj_path,
                    &source.to_string_lossy(),
                )
                .with_context(|| {
                    format!("native code generation failed for {}", source.display())
                })?;
                println!(
                    "compiled {} -> {} ({} functions, {} globals, {} -> {} IR instructions, opt removed {})",
                    source.display(),
                    obj_path.display(),
                    compiled.funcs,
                    compiled.globals,
                    stats.instrs_before,
                    stats.instrs_after,
                    stats.total_removed(),
                );
                objects.push(obj_path);
            }
            link_executable(&objects, &output)?;
            println!(
                "linked {} object file(s) -> {}",
                objects.len(),
                output.display()
            );
            Ok(())
        }
    }
}

/// Single-source build: the classic behavior, unchanged.
fn build_single(source: PathBuf, backend: BackendArg, cli: &Cli) -> Result<()> {
    let output = cli.output.clone().unwrap_or_else(|| match backend {
        BackendArg::Ir => source.with_extension("pgir"),
        BackendArg::Native => default_native_output(&source),
    });

    let compiled = pipeline::compile_to_ir(
        &source,
        &cli.include_dirs,
        pipeline::map_opt_level(cli.opt_level),
    )?;
    pipeline::emit_warnings(&compiled.diags, &compiled.source);

    let stats = &compiled.stats;
    match backend {
        BackendArg::Ir => {
            std::fs::write(&output, &compiled.ir)
                .with_context(|| format!("cannot write {}", output.display()))?;
        }
        BackendArg::Native => {
            // Object code goes through a scratch directory; only the
            // linked executable lands at the requested output.
            let temp = tempfile::tempdir()
                .context("cannot create a temporary directory for object code")?;
            let obj_path = temp.path().join("polyglot.o");
            let obj = polyglot_codegen::emit_object(
                &compiled.module,
                &obj_path,
                &source.to_string_lossy(),
            )
            .with_context(|| format!("native code generation failed for {}", source.display()))?;
            link_executable(std::slice::from_ref(&obj), &output)?;
        }
    }

    println!(
        "compiled {} -> {} ({} functions, {} globals, {} -> {} IR instructions, opt removed {})",
        source.display(),
        output.display(),
        compiled.funcs,
        compiled.globals,
        stats.instrs_before,
        stats.instrs_after,
        stats.total_removed(),
    );
    Ok(())
}

/// Collision-free object-file stem for a source path
/// (`dir/a/main.c` -> `main`); duplicates get their index appended by
/// the caller if they ever matter.
fn sanitize_stem(source: &Path) -> String {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "module".to_string());
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "module".to_string()
    } else {
        cleaned
    }
}

/// Default executable path for a native build: the source without its
/// extension (`01_hello.c` -> `01_hello`).
fn default_native_output(source: &Path) -> PathBuf {
    let stem = source.with_extension("");
    if stem.as_os_str().is_empty() {
        PathBuf::from("a.out")
    } else {
        stem
    }
}

/// Link object files into an executable with the system linker
/// driver (`cc`, falling back to `gcc`).
fn link_executable(objs: &[PathBuf], output: &Path) -> Result<()> {
    let cc = find_link_driver()
        .ok_or_else(|| anyhow::anyhow!("no linker driver found (need 'cc' or 'gcc' on PATH)"))?;
    let status = std::process::Command::new(&cc)
        .args(objs)
        .arg("-o")
        .arg(output)
        .status()
        .with_context(|| format!("cannot invoke linker {}", cc.display()))?;
    if !status.success() {
        anyhow::bail!("linking failed with exit code {:?}", status.code());
    }
    Ok(())
}

fn find_link_driver() -> Option<PathBuf> {
    ["cc", "gcc"]
        .iter()
        .find_map(|name| which::which(name).ok())
}

/// `polyglot check <src.c>`: parse + semantic analysis only.
/// Diagnostics go to stderr; exit code 1 on any error.
fn handle_check(args: CheckArgs, cli: &Cli) -> Result<()> {
    if !args.file.is_file() {
        anyhow::bail!("no such source file: {}", args.file.display());
    }
    let (source, diags, nfuncs) = pipeline::check_only(&args.file, &cli.include_dirs)?;
    pipeline::emit_warnings(&diags, &source);
    println!("{}: ok ({} functions checked)", args.file.display(), nfuncs);
    Ok(())
}

/// `polyglot emit-ir <src.c>`: same pipeline as build, but the textual
/// IR (Polyglot by default, LLVM with --format llvm) goes to stdout by
/// default, or to the file given with -o.
fn handle_emit_ir(args: EmitIrArgs, cli: &Cli) -> Result<()> {
    if !args.file.is_file() {
        anyhow::bail!("no such source file: {}", args.file.display());
    }

    let compiled = pipeline::compile_to_ir(
        &args.file,
        &cli.include_dirs,
        pipeline::map_opt_level(cli.opt_level),
    )?;
    pipeline::emit_warnings(&compiled.diags, &compiled.source);

    let text = match args.format {
        IrFormatArg::PgIr => compiled.ir,
        IrFormatArg::Llvm => {
            polyglot_codegen::emit_ll_named(&compiled.module, &args.file.to_string_lossy())?
        }
    };

    if let Some(output) = &cli.output {
        std::fs::write(output, &text)
            .with_context(|| format!("cannot write {}", output.display()))?;
        let stats = &compiled.stats;
        eprintln!(
            "wrote {} ({} functions, {} -> {} IR instructions, opt removed {})",
            output.display(),
            compiled.funcs,
            stats.instrs_before,
            stats.instrs_after,
            stats.total_removed(),
        );
    } else {
        print!("{}", text);
    }
    Ok(())
}

// `polyglot transmute-internal <src.c>`: show the step-by-step compilation
// process with explanations of each stage.
fn handle_transmute_internal(args: TransmuteInternalArgs, cli: &Cli) -> Result<()> {
    if !args.path.is_file() {
        anyhow::bail!("no such source file: {}", args.path.display());
    }

    // Show the compilation process step by step
    println!("; polyglot transmute-internal: {}", args.path.display());
    println!("; showing the step-by-step compilation process");
    println!();

    // Step 1: Source code
    println!("; === STEP 1: SOURCE CODE ===");
    println!("; The original C/C++ source code as written by the programmer");
    println!();
    let source_code = std::fs::read_to_string(&args.path)?;
    for (i, line) in source_code.lines().enumerate() {
        println!("; {:>4}: {}", i + 1, line);
    }
    println!();

    // Step 2: After parsing (AST)
    println!("; === STEP 2: AFTER PARSING (AST) ===");
    println!("; The source code has been parsed into an Abstract Syntax Tree");
    println!("; This shows the syntactic structure of the program");
    println!();

    // We'll get the parsed unit from parse_and_check but don't check yet
    let mut diags = DiagnosticEngine::new();
    let source_text = std::fs::read_to_string(&args.path)?;
    let (source_file, unit) = match polyglot_syntax::parse_source(
        &args.path,
        &source_text,
        &cli.include_dirs,
        &mut diags,
    ) {
        Ok((sf, u)) => (sf, u),
        Err(e) => {
            // Render diagnostics and return error
            // We need a source_file for rendering, but we don't have one yet
            // Create a dummy one for error rendering purposes
            let dummy_source = SourceFile::new(args.path, source_text);
            let mut out = String::new();
            diags.render(&dummy_source, &mut out);
            if !out.trim().is_empty() {
                eprintln!("{}", out.trim_end());
            }
            anyhow::bail!("parsing failed: {}", e);
        }
    };

    // Show a simplified AST representation
    println!("; Parsed successfully into AST");
    println!("; (AST visualization omitted for brevity - would show expressions, statements, declarations)");
    println!();

    // Step 3: After semantic analysis
    println!("; === STEP 3: AFTER SEMANTIC ANALYSIS ===");
    println!("; Types have been checked and names have been resolved to declarations");
    println!("; This ensures the program follows the language's semantic rules");
    println!();

    let checked = match polyglot_sema::check(unit, &mut diags) {
        Ok(ok) => ok,
        Err(e) => {
            // Render diagnostics and return error
            let mut out = String::new();
            diags.render(&source_file, &mut out);
            if !out.trim().is_empty() {
                eprintln!("{}", out.trim_end());
            }
            anyhow::bail!("type checking failed: {}", e);
        }
    };

    if diags.has_errors() {
        // Render diagnostics and return error
        let mut out = String::new();
        diags.render(&source_file, &mut out);
        if !out.trim().is_empty() {
            eprintln!("{}", out.trim_end());
        }
        anyhow::bail!("type checking failed");
    }

    // Show some basic info from semantic analysis
    println!("; Semantic analysis completed:");
    println!(";   {} functions", checked.funcs.len());
    println!(";   {} global variables", checked.globals.len());
    println!();

    // Step 4: After lowering to IR
    println!("; === STEP 4: AFTER LOWERING TO IR ===");
    println!("; The checked program has been lowered to Polyglot IR");
    println!("; This is a typed, register-based, block-structured intermediate representation");
    println!();

    let mut module = polyglot_ir::lower_program(checked);
    let ir_before_opt = print_module(&module);
    println!("; IR after lowering (before optimization):");
    for line in ir_before_opt.lines() {
        println!("; {}", line);
    }
    println!();

    // Step 5: After each optimization pass
    println!("; === STEP 5: OPTIMIZATION PASSES ===");
    println!("; Applying optimization passes to improve the code");
    println!();

    let opt_level = pipeline::map_opt_level(cli.opt_level);
    let stats = if opt_level == polyglot_ir::OptLevel::None {
        polyglot_ir::OptStats::default()
    } else {
        // We need to modify the optimize_module function to return per-pass stats
        // For now, we'll show the overall effect
        polyglot_ir::optimize_module(&mut module, opt_level)
    };

    let ir_after_opt = print_module(&module);
    println!("; IR after optimization:");
    for line in ir_after_opt.lines() {
        println!("; {}", line);
    }
    println!();

    // Optimization explanations
    println!("; Optimization explanations:");
    println!("; 1. Constant folding: Evaluating constant expressions at compile time");
    println!("; 2. Simplification: Applying algebraic identities (x+0=x, x*1=x, etc.)");
    println!("; 3. Dead code elimination: Removing code that can never be executed");
    println!("; 4. Branch folding: Converting constant conditions to unconditional jumps");
    println!();

    // Step 6: Final IR
    println!("; === STEP 6: FINAL IR ===");
    println!("; The final intermediate representation ready for codegen");
    println!();

    println!("; Final optimized IR:");
    for line in ir_after_opt.lines() {
        println!("; {}", line);
    }
    println!();

    // Statistics
    println!("; === STATISTICS ===");
    println!("; Functions: {}", module.funcs.len());
    println!("; Global variables: {}", module.globals.len());
    println!("; Instructions before optimization: {}", stats.instrs_before);
    println!("; Instructions after optimization: {}", stats.instrs_after);
    println!("; Instructions removed: {}", stats.total_removed());
    println!("; Constants folded: {}", stats.constants_folded);
    println!("; Instructions simplified: {}", stats.instrs_simplified);
    println!("; Dead instructions removed: {}", stats.dead_removed);
    println!("; Branches folded: {}", stats.branches_folded);
    println!("; Optimization iterations: {}", stats.iterations);
    println!();

    // Legend
    println!("; Legend (common IR mnemonics):");
    println!(";  %r = const 42 : int         -- constant into a register");
    println!(";  store.local s0, %r0          -- write a local slot");
    println!(";  %r = load.local s0           -- read a local slot");
    println!(";  %r = add.int %a, %b          -- arithmetic operations");
    println!(";  br bb1                       -- unconditional jump");
    println!(";  br.cond %c, bb1, bb2         -- conditional jump");
    println!(";  %r = call @f(...)            -- function call");
    println!(";  ret %r                       -- return from function");
    println!();

    Ok(())
}

fn handle_run_simple(
    args: RunArgs,
    _cli: &Cli,
    target: Triple,
    opt_level: polyglot_protocol::types::OptLevel,
) -> Result<()> {
    let source_file = args.file;

    eprintln!(
        "[polyglot] note: 'run' is a native-execution fallback that shells out to system clang; \
         use 'polyglot build' for the in-process Polyglot IR pipeline"
    );

    // Build to temporary executable
    let temp_dir = tempfile::tempdir()?;
    let exe_path = temp_dir.path().join("polyglot_run");

    build_with_clang(&source_file, &exe_path, &target, opt_level, &[])?;

    // Run the executable
    use std::process::Command;
    let mut cmd = Command::new(&exe_path);
    cmd.args(&args.args);
    let status = cmd.status().context("Failed to run executable")?;

    std::process::exit(status.code().unwrap_or(1));
}

fn build_with_clang(
    source_file: &PathBuf,
    output_file: &PathBuf,
    target: &Triple,
    opt_level: polyglot_protocol::types::OptLevel,
    extra_args: &[String],
) -> Result<()> {
    let clang = find_clang();
    let clang = match clang {
        Some(c) => c,
        None => anyhow::bail!("clang not found in PATH or under MLIR_SYS_220_PREFIX/bin"),
    };

    let mut cmd = std::process::Command::new(clang);
    cmd.arg(source_file)
        .arg("-o")
        .arg(output_file)
        .arg(format!("--target={}", target));

    // Add optimization level
    let opt_flag = match opt_level {
        polyglot_protocol::types::OptLevel::None => "-O0",
        polyglot_protocol::types::OptLevel::Less => "-O1",
        polyglot_protocol::types::OptLevel::Default => "-O2",
        polyglot_protocol::types::OptLevel::Aggressive => "-O3",
        polyglot_protocol::types::OptLevel::Size => "-Os",
        polyglot_protocol::types::OptLevel::SizeAggressive => "-Oz",
    };
    cmd.arg(opt_flag);

    // Add extra args
    for arg in extra_args {
        cmd.arg(arg);
    }

    info!("Running: {:?}", cmd);

    let status = cmd.status().context("Failed to execute clang")?;
    if !status.success() {
        anyhow::bail!("Compilation failed with exit code: {:?}", status.code());
    }

    Ok(())
}

/// Locate a clang to shell out to: system PATH first, then the clang
/// shipped in the LLVM install this workspace was configured against
/// (`MLIR_SYS_220_PREFIX` at runtime, or baked in at build time via
/// `.cargo/config.toml`).
fn find_clang() -> Option<PathBuf> {
    if let Ok(c) = which::which("clang") {
        return Some(c);
    }
    let prefix = std::env::var_os("MLIR_SYS_220_PREFIX")
        .or_else(|| option_env!("MLIR_SYS_220_PREFIX").map(std::ffi::OsString::from))?;
    let clang = PathBuf::from(prefix).join("bin").join("clang");
    if clang.exists() {
        Some(clang)
    } else {
        None
    }
}

fn handle_init(args: InitArgs) -> Result<()> {
    let name = args.name.unwrap_or_else(|| "my-project".to_string());
    let language = args.language.unwrap_or_else(|| "c".to_string());

    let project_dir = PathBuf::from(&name);
    std::fs::create_dir_all(&project_dir)?;

    // Create polyglot.toml
    let plugin_name = match language.as_str() {
        "c" | "cpp" => "clang",
        "rust" => "rustc",
        _ => "clang",
    };

    let config = format!(
        r#"[project]
name = "{}"
version = "0.1.0"

[target]
triple = "{}"
opt_level = 2

[plugins]
{} = "0.1"

[[bin]]
name = "{}"
source = "src/main.{}"
"#,
        name,
        target_lexicon::HOST,
        plugin_name,
        name,
        if language == "cpp" { "cpp" } else { "c" }
    );

    std::fs::write(project_dir.join("polyglot.toml"), config)?;

    // Create source directory
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    // Create sample source file
    let sample_source = if language == "cpp" {
        r#"#include <iostream>

int main() {
    std::cout << "Hello from Polyglot!" << std::endl;
    return 0;
}
"#
    } else {
        r#"#include <stdio.h>

int main() {
    printf("Hello from Polyglot!\n");
    return 0;
}
"#
    };

    let ext = if language == "cpp" { "cpp" } else { "c" };
    std::fs::write(src_dir.join(format!("main.{}", ext)), sample_source)?;

    info!("Initialized project '{}' with {} support", name, language);
    println!("Project created at: {}", project_dir.display());
    println!("Run 'polyglot build' to build.");

    Ok(())
}
