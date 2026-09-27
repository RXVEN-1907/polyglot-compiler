use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use target_lexicon::Triple;

#[derive(Parser)]
#[command(name = "polyglot", version, about = "Universal compiler driver")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Target triple
    #[arg(long, global = true, value_name = "TRIPLE")]
    pub target: Option<String>,

    /// Optimization level
    #[arg(
        long,
        short = 'O',
        global = true,
        value_enum,
        default_value = "default"
    )]
    pub opt_level: OptLevelArg,

    /// Add a directory to the include search path
    #[arg(short = 'I', long = "include", global = true, value_name = "DIR")]
    pub include_dirs: Vec<PathBuf>,

    /// Output file
    #[arg(long, short, global = true, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Plugin to use
    #[arg(long, global = true, value_name = "NAME")]
    pub plugin: Option<String>,

    /// Verbose output
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Plugin directory
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        default_value = "~/.polyglot/plugins"
    )]
    pub plugin_dir: PathBuf,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Compile a C source file (to a textual IR artifact, or fully
    /// native with --backend native)
    Build(BuildArgs),

    /// Parse and type-check a C source file, printing diagnostics
    Check(CheckArgs),

    /// Compile a C source file and print textual IR (Polyglot or LLVM)
    EmitIr(EmitIrArgs),

    /// Run a source file directly
    Run(RunArgs),

    /// Manage plugins
    Plugin(PluginArgs),

    /// Initialize a new project
    Init(InitArgs),

    /// Show version info
    Version,
}

#[derive(Parser, Clone)]
pub struct BuildArgs {
    /// Source file(s) or project directory
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Code generation backend
    #[arg(long, value_enum, default_value = "ir", value_name = "BACKEND")]
    pub backend: BackendArg,

    /// Compiler arguments to pass to frontend
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<String>,
}

#[derive(Parser, Clone)]
pub struct RunArgs {
    /// Source file to run
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Arguments to pass to the program
    #[arg(last = true, value_name = "ARGS")]
    pub args: Vec<String>,
}

#[derive(Parser, Clone)]
pub struct CheckArgs {
    /// Source file to check
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
}

#[derive(Parser, Clone)]
pub struct EmitIrArgs {
    /// Source file to compile
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// IR dialect to emit
    #[arg(long, value_enum, default_value = "pgir", value_name = "FORMAT")]
    pub format: IrFormatArg,
}

#[derive(Parser, Clone)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: PluginCommand,
}

#[derive(Subcommand, Clone)]
pub enum PluginCommand {
    /// Add a plugin
    Add {
        /// Plugin specification (name@version)
        #[arg(value_name = "SPEC")]
        spec: String,
    },
    /// Remove a plugin
    Remove {
        /// Plugin name
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// List installed plugins
    List,
    /// Update plugins
    Update,
}

#[derive(Parser, Clone)]
pub struct InitArgs {
    /// Project name
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Language/plugin to use
    #[arg(long, value_name = "LANG")]
    pub language: Option<String>,
}

/// Code generation backend for `polyglot build`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum BackendArg {
    /// Textual Polyglot IR artifact (default; parse -> check -> lower
    /// -> optimize -> print).
    #[value(name = "ir")]
    Ir,
    /// Native executable: the same frontend pipeline, then LLVM object
    /// emission via `llc` and linking via the system `cc`. Single
    /// source file only — no multi-TU linking yet.
    #[value(name = "native")]
    Native,
}

/// IR dialect for `polyglot emit-ir`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum IrFormatArg {
    /// Polyglot IR (default).
    #[value(name = "pgir")]
    PgIr,
    /// LLVM 17 textual IR.
    #[value(name = "llvm")]
    Llvm,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum OptLevelArg {
    None,
    Less,
    Default,
    Aggressive,
    Size,
    SizeAggressive,
}

impl From<OptLevelArg> for polyglot_protocol::types::OptLevel {
    fn from(arg: OptLevelArg) -> Self {
        match arg {
            OptLevelArg::None => polyglot_protocol::types::OptLevel::None,
            OptLevelArg::Less => polyglot_protocol::types::OptLevel::Less,
            OptLevelArg::Default => polyglot_protocol::types::OptLevel::Default,
            OptLevelArg::Aggressive => polyglot_protocol::types::OptLevel::Aggressive,
            OptLevelArg::Size => polyglot_protocol::types::OptLevel::Size,
            OptLevelArg::SizeAggressive => polyglot_protocol::types::OptLevel::SizeAggressive,
        }
    }
}

impl Cli {
    pub fn target_triple(&self) -> Result<Triple, target_lexicon::ParseError> {
        if let Some(target) = &self.target {
            target.parse()
        } else {
            // Default to host triple
            Ok(target_lexicon::HOST.to_string().parse()?)
        }
    }
}
