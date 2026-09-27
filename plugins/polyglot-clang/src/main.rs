//! Polyglot Clang Plugin - C/C++ frontend via Clang's MLIR emission

use anyhow::{Context, Result};
use polyglot_protocol::{
    handler::{Plugin, PluginServer},
    transport::ServerTransport,
    types::{
        CompileParams, CompileResult, InitializeParams, InitializeResult, PluginCapabilities,
        PluginInfo,
    },
};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;
use tracing::{debug, info, warn};

struct ClangPlugin {
    info: PluginInfo,
    capabilities: PluginCapabilities,
}

impl ClangPlugin {
    fn new() -> Self {
        Self {
            info: PluginInfo {
                name: "clang".to_string(),
                version: "0.1.0".to_string(),
                description: "C/C++ frontend via Clang".to_string(),
                author: "Polyglot Team".to_string(),
            },
            capabilities: PluginCapabilities {
                mlir_dialects: vec![
                    "llvm".to_string(),
                    "affine".to_string(),
                    "arith".to_string(),
                    "func".to_string(),
                    "memref".to_string(),
                    "scf".to_string(),
                ],
                source_extensions: vec![
                    "c".to_string(),
                    "h".to_string(),
                    "cpp".to_string(),
                    "cc".to_string(),
                    "cxx".to_string(),
                    "hpp".to_string(),
                ],
                languages: vec!["c".to_string(), "c++".to_string()],
                incremental: false,
            },
        }
    }

    fn find_clang(&self) -> Result<PathBuf> {
        // System PATH first; otherwise fall back to the clang shipped in
        // the LLVM install this workspace was configured against
        // (`MLIR_SYS_220_PREFIX`, see .cargo/config.toml).
        if let Ok(c) = which::which("clang") {
            return Ok(c);
        }
        let prefix = std::env::var_os("MLIR_SYS_220_PREFIX")
            .or_else(|| option_env!("MLIR_SYS_220_PREFIX").map(std::ffi::OsString::from));
        if let Some(prefix) = prefix {
            let clang = PathBuf::from(prefix).join("bin").join("clang");
            if clang.exists() {
                return Ok(clang);
            }
        }
        anyhow::bail!("clang not found in PATH or under MLIR_SYS_220_PREFIX/bin")
    }

    async fn compile_to_mlir(
        &self,
        source_path: &str,
        args: &[String],
        target_triple: &str,
    ) -> Result<String> {
        // `-emit-mlir` only exists in MLIR-enabled clang builds (e.g. the
        // polyglot-llvm fork); stock clang rejects it. It is therefore
        // opt-in via POLYGLOT_CLANG_EMIT_MLIR=1. The default emits plain
        // LLVM IR text via `-S -emit-llvm`, which every clang supports.
        if std::env::var_os("POLYGLOT_CLANG_EMIT_MLIR").is_some() {
            return self
                .emit(source_path, args, target_triple, &["-emit-mlir"], "MLIR")
                .await;
        }
        warn!(
            "stock clang has no -emit-mlir; emitting LLVM IR text via -S -emit-llvm \
             (set POLYGLOT_CLANG_EMIT_MLIR=1 to require MLIR emission from an \
             MLIR-enabled clang)"
        );
        self.emit(source_path, args, target_triple, &["-emit-llvm"], "LLVM IR")
            .await
    }

    async fn emit(
        &self,
        source_path: &str,
        args: &[String],
        target_triple: &str,
        emit_flag: &[&str],
        what: &str,
    ) -> Result<String> {
        let clang = self.find_clang()?;

        // Temporary file for the emitted text
        let out_file = NamedTempFile::new()?;
        let out_path = out_file.path().to_path_buf();

        let mut cmd = Command::new(&clang);
        for flag in emit_flag {
            cmd.arg(flag);
        }
        cmd.arg("-S")
            .arg("-o")
            .arg(&out_path)
            .arg(source_path)
            .arg(format!("--target={}", target_triple));

        for arg in args {
            cmd.arg(arg);
        }

        debug!("Running clang command: {:?}", cmd);

        let output = cmd.output().context("Failed to execute clang")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("clang {} emission failed: {}", what, stderr);
        }

        let text = tokio::fs::read_to_string(&out_path)
            .await
            .context("Failed to read clang output")?;

        Ok(text)
    }
}

#[async_trait::async_trait]
impl Plugin for ClangPlugin {
    fn info(&self) -> PluginInfo {
        self.info.clone()
    }

    fn capabilities(&self) -> PluginCapabilities {
        self.capabilities.clone()
    }

    async fn initialize(&mut self, _params: InitializeParams) -> Result<InitializeResult> {
        info!("Initializing Clang plugin");
        Ok(InitializeResult {
            protocol_version: polyglot_protocol::types::PROTOCOL_VERSION.to_string(),
            capabilities: self.capabilities(),
            plugin_info: self.info(),
        })
    }

    async fn compile(&mut self, params: CompileParams) -> Result<CompileResult> {
        info!("Compiling: {}", params.source_path);

        // Handle source content if provided directly
        let source_path = if params.source_path == "-" {
            if let Some(content) = params.source_content {
                let mut temp_file = NamedTempFile::new()?;
                temp_file.write_all(content.as_bytes())?;
                temp_file.path().to_string_lossy().to_string()
            } else {
                anyhow::bail!("No source content provided for stdin input");
            }
        } else {
            params.source_path.clone()
        };

        // Compile to MLIR
        let mlir_module = self
            .compile_to_mlir(&source_path, &params.args, &params.target_triple)
            .await?;

        // Parse diagnostics from clang output if needed
        let diagnostics = Vec::new();

        Ok(CompileResult {
            mlir_module,
            diagnostics,
            cache_key: None,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging on stderr: stdout is the JSON-RPC stream, and any
    // log line written there would corrupt the newline-delimited protocol.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let plugin = ClangPlugin::new();
    let mut server = PluginServer::new(plugin);
    let mut transport = ServerTransport::new();

    info!("Clang plugin started");

    loop {
        match transport.read_request().await {
            Ok(Some(request)) => {
                let response = server.handle_request(request).await?;
                transport.send_response(response).await?;
            }
            Ok(None) => {
                // EOF
                break;
            }
            Err(e) => {
                eprintln!("Error reading request: {}", e);
                break;
            }
        }
    }

    info!("Clang plugin shutting down");
    Ok(())
}
