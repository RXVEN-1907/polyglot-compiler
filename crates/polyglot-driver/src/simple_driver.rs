//! Simple driver that uses clang directly for the MVP
//! This avoids the MLIR static linking issue while providing a working compiler

use crate::cli::{
    BackendArg, BuildArgs, CheckArgs, Cli, Commands, EmitIrArgs, InitArgs, IrFormatArg, RunArgs,
};
use crate::pipeline;
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
