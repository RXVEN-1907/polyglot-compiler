//! Polyglot Clang Plugin - C/C++ frontend via Clang's MLIR emission

use anyhow::{Context, Result};
use polyglot_protocol::{
    handler::{Plugin, PluginServer},
    transport::ServerTransport,
    types::{
        CompileParams, CompileResult, InitializeParams,
        InitializeResult, PluginCapabilities, PluginInfo,
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
        which::which("clang").context("clang not found in PATH")
    }

    async fn compile_to_mlir(
        &self,
        source_path: &str,
        args: &[String],
        target_triple: &str,
    ) -> Result<String> {
        let clang = self.find_clang()?;

        // Create temporary file for MLIR output
        let mlir_output = NamedTempFile::new()?;
        let mlir_path = mlir_output.path().to_path_buf();

        // Build clang command
        let mut cmd = Command::new(&clang);
        cmd.arg("-emit-mlir")
            .arg("-S")
            .arg("-o")
            .arg(&mlir_path)
            .arg(source_path);

        // Add target triple
        cmd.arg(format!("--target={}", target_triple));

        // Add user args
        for arg in args {
            cmd.arg(arg);
        }

        debug!("Running clang command: {:?}", cmd);

        let output = cmd.output().context("Failed to execute clang")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Try without -emit-mlir as fallback
            warn!("clang -emit-mlir failed, trying alternative: {}", stderr);
            return self.compile_to_mlir_fallback(source_path, args, target_triple).await;
        }

        // Read MLIR output
        let mlir_text = tokio::fs::read_to_string(&mlir_path).await
            .context("Failed to read MLIR output")?;

        Ok(mlir_text)
    }

    async fn compile_to_mlir_fallback(
        &self,
        source_path: &str,
        args: &[String],
        target_triple: &str,
    ) -> Result<String> {
        let clang = self.find_clang()?;

        // Try with -Xclang -emit-mlir
        let mlir_output = NamedTempFile::new()?;
        let mlir_path = mlir_output.path().to_path_buf();

        let mut cmd = Command::new(&clang);
        cmd.arg("-Xclang")
            .arg("-emit-mlir")
            .arg("-S")
            .arg("-o")
            .arg(&mlir_path)
            .arg(source_path);

        cmd.arg(format!("--target={}", target_triple));

        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd.output().context("Failed to execute clang (fallback)")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("clang MLIR emission failed: {}", stderr);
        }

        let mlir_text = tokio::fs::read_to_string(&mlir_path).await
            .context("Failed to read MLIR output")?;

        Ok(mlir_text)
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
    // Initialize logging
    tracing_subscriber::fmt::init();

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