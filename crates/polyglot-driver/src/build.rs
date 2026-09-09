use crate::types::{BuildConfig, BinConfig, CompilationUnit, OutputType, ProjectConfig};
use crate::plugin_manager::PluginManager;
use anyhow::{Context, Result};
use polyglot_codegen::Codegen;
use polyglot_mlir::MlirContext;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use target_lexicon::Triple;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Build orchestrator
pub struct BuildOrchestrator {
    pub config: BuildConfig,
    plugin_manager: Arc<PluginManager>,
    mlir_context: Arc<MlirContext>,
    codegen: Arc<Mutex<Codegen>>,
    cache_dir: Option<PathBuf>,
}

impl BuildOrchestrator {
    pub async fn new(config: BuildConfig, plugin_manager: Arc<PluginManager>) -> Result<Self> {
        let mlir_context = Arc::new(MlirContext::new()?);
        let codegen = Arc::new(Mutex::new(Codegen::new(config.target.clone())?));

        Ok(Self {
            config,
            plugin_manager,
            mlir_context,
            codegen,
            cache_dir: None,
        })
    }

    /// Build a project from polyglot.toml
    pub async fn build_project(&mut self, project: ProjectConfig) -> Result<Vec<PathBuf>> {
        let mut outputs = Vec::new();

        for bin in project.bins {
            let output = self.build_binary(&bin).await?;
            outputs.push(output);
        }

        Ok(outputs)
    }

    /// Build a single binary
    pub async fn build_binary(&mut self, bin: &BinConfig) -> Result<PathBuf> {
        info!("Building binary: {}", bin.name);

        // Determine plugin
        let plugin_name = bin.plugin.as_ref()
            .unwrap_or(&self.config.plugins[0].name);

        let plugin_spec = self.config.plugins.iter()
            .find(|p| p.name == *plugin_name)
            .context(format!("Plugin not found: {}", plugin_name))?;

        // Get plugin instance
        let plugin_instance = self.plugin_manager.get_plugin(plugin_spec).await?;

        // Initialize plugin if needed
        let capabilities = polyglot_protocol::types::PluginCapabilities {
            mlir_dialects: vec!["llvm".to_string(), "affine".to_string(), "arith".to_string()],
            source_extensions: vec!["c".to_string(), "h".to_string()],
            languages: vec!["c".to_string()],
            incremental: false,
        };

        self.plugin_manager.initialize_plugin(
            plugin_instance.clone(),
            &self.config.target,
            capabilities,
        ).await?;

        // Compile source to MLIR
        let compile_result = self.plugin_manager.compile(
            plugin_instance,
            &bin.source,
            &bin.args,
            &self.config.target,
            self.config.optimization,
        ).await?;

        // Parse MLIR
        let mlir_module = self.mlir_context.parse_module(&compile_result.mlir_module)?;

        // Optimize MLIR - convert OptLevel type
        let mlir_opt_level = match self.config.optimization {
            polyglot_protocol::types::OptLevel::None => polyglot_mlir::types::OptLevel::None,
            polyglot_protocol::types::OptLevel::Less => polyglot_mlir::types::OptLevel::Less,
            polyglot_protocol::types::OptLevel::Default => polyglot_mlir::types::OptLevel::Default,
            polyglot_protocol::types::OptLevel::Aggressive => polyglot_mlir::types::OptLevel::Aggressive,
            polyglot_protocol::types::OptLevel::Size => polyglot_mlir::types::OptLevel::Size,
            polyglot_protocol::types::OptLevel::SizeAggressive => polyglot_mlir::types::OptLevel::SizeAggressive,
        };
        self.mlir_context.optimize_module(&mlir_module, mlir_opt_level)?;

        // Codegen to object file
        let mut codegen = self.codegen.lock().await;
        let object_file = codegen.emit_object(&mlir_module, &bin.name)?;

        // Link executable
        let output_path = self.link_executable(&bin.name, &[object_file])?;

        Ok(output_path)
    }

    /// Build a single source file directly
    pub async fn build_file(&mut self, source_path: &PathBuf, output_path: &PathBuf, plugin_name: &str) -> Result<()> {
        info!("Building file: {:?} -> {:?}", source_path, output_path);

        let plugin_spec = self.config.plugins.iter()
            .find(|p| p.name == plugin_name)
            .context(format!("Plugin not found: {}", plugin_name))?;

        let plugin_instance = self.plugin_manager.get_plugin(plugin_spec).await?;

        let capabilities = polyglot_protocol::types::PluginCapabilities {
            mlir_dialects: vec!["llvm".to_string(), "affine".to_string(), "arith".to_string()],
            source_extensions: vec!["c".to_string(), "h".to_string()],
            languages: vec!["c".to_string()],
            incremental: false,
        };

        self.plugin_manager.initialize_plugin(
            plugin_instance.clone(),
            &self.config.target,
            capabilities,
        ).await?;

        let compile_result = self.plugin_manager.compile(
            plugin_instance,
            source_path,
            &[],
            &self.config.target,
            self.config.optimization,
        ).await?;

        let mlir_module = self.mlir_context.parse_module(&compile_result.mlir_module)?;
        // Optimize MLIR - convert OptLevel type
        let mlir_opt_level = match self.config.optimization {
            polyglot_protocol::types::OptLevel::None => polyglot_mlir::types::OptLevel::None,
            polyglot_protocol::types::OptLevel::Less => polyglot_mlir::types::OptLevel::Less,
            polyglot_protocol::types::OptLevel::Default => polyglot_mlir::types::OptLevel::Default,
            polyglot_protocol::types::OptLevel::Aggressive => polyglot_mlir::types::OptLevel::Aggressive,
            polyglot_protocol::types::OptLevel::Size => polyglot_mlir::types::OptLevel::Size,
            polyglot_protocol::types::OptLevel::SizeAggressive => polyglot_mlir::types::OptLevel::SizeAggressive,
        };
        self.mlir_context.optimize_module(&mlir_module, mlir_opt_level)?;

        let mut codegen = self.codegen.lock().await;
        let object_file = codegen.emit_object(&mlir_module, "main")?;

        self.link_executable(output_path.to_str().unwrap(), &[object_file])?;

        Ok(())
    }

    fn link_executable(&self, output_name: &str, object_files: &[PathBuf]) -> Result<PathBuf> {
        use std::process::Command;

        let output_path = PathBuf::from(output_name);

        let mut cmd = Command::new("clang");
        cmd.arg("-o").arg(&output_path);
        for obj in object_files {
            cmd.arg(obj);
        }

        let status = cmd.status().context("Failed to run linker")?;
        if !status.success() {
            anyhow::bail!("Linking failed");
        }

        Ok(output_path)
    }
}