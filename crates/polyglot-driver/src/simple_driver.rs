//! Simple driver that uses clang directly for the MVP
//! This avoids the MLIR static linking issue while providing a working compiler

use crate::cli::{BuildArgs, Cli, Commands, InitArgs, RunArgs};
use crate::types::ProjectConfig;
use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use target_lexicon::Triple;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

pub fn run_simple_driver(cli: Cli) -> Result<()> {
    // Setup logging
    let level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Parse target triple
    let target = cli.target_triple().context("Invalid target triple")?;

    // Parse optimization level
    let opt_level: polyglot_protocol::types::OptLevel = cli.opt_level.into();

    // Execute command
    match cli.command {
        Commands::Build(args) => handle_build_simple(args, &cli, target, opt_level)?,
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

fn handle_build_simple(
    args: BuildArgs,
    cli: &Cli,
    target: Triple,
    opt_level: polyglot_protocol::types::OptLevel,
) -> Result<()> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));

    // Determine output path
    let output = cli.output.clone().unwrap_or_else(|| {
        if path.is_file() {
            path.with_extension("")
        } else {
            PathBuf::from("a.out")
        }
    });

    // Determine source file
    let source_file = if path.is_file() {
        path.clone()
    } else if path.join("polyglot.toml").exists() {
        // For project builds, find the first source file
        let project_config = load_project_config(&path)?;
        project_config.bins.first()
            .map(|b| path.join(&b.source))
            .unwrap_or_else(|| PathBuf::from("src/main.c"))
    } else {
        anyhow::bail!("No source file or project found at {:?}", path);
    };

    // Build using clang directly
    build_with_clang(&source_file, &output, &target, opt_level, &args.args)?;

    info!("Built {:?} -> {:?}", source_file, output);
    Ok(())
}

fn handle_run_simple(
    args: RunArgs,
    cli: &Cli,
    target: Triple,
    opt_level: polyglot_protocol::types::OptLevel,
) -> Result<()> {
    let source_file = args.file;

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
    let clang = which::which("clang").context("clang not found in PATH")?;

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

    let config = format!(r#"[project]
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
"#, name, target_lexicon::HOST, plugin_name, name, if language == "cpp" { "cpp" } else { "c" });

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

fn load_project_config(path: &PathBuf) -> Result<ProjectConfig> {
    let config_path = path.join("polyglot.toml");
    let content = std::fs::read_to_string(config_path)?;
    let config: ProjectConfig = toml::from_str(&content)?;
    Ok(config)
}