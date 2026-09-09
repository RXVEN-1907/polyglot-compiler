use crate::types::{PluginManifest, PluginSource, PluginSpec};
use anyhow::{Context, Result};
use polyglot_protocol::transport::StdioTransport;
use polyglot_protocol::types::{CompileParams, CompileResult, PluginCapabilities};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use target_lexicon::Triple;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Manages plugin lifecycle and communication
pub struct PluginManager {
    plugins: Arc<Mutex<HashMap<String, Arc<Mutex<PluginInstance>>>>>,
    pub plugin_dir: PathBuf,
}

pub struct PluginInstance {
    manifest: PluginManifest,
    transport: Option<StdioTransport>,
    initialized: bool,
}

impl std::fmt::Debug for PluginInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginInstance")
            .field("manifest", &self.manifest)
            .field("initialized", &self.initialized)
            .finish()
    }
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Arc::new(Mutex::new(HashMap::new())),
            plugin_dir,
        }
    }

    /// Discover and load available plugins
    pub async fn discover_plugins(&self) -> Result<Vec<PluginManifest>> {
        let mut manifests = Vec::new();

        if !self.plugin_dir.exists() {
            return Ok(manifests);
        }

        let mut entries = tokio::fs::read_dir(&self.plugin_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("polyglot-plugin.toml");
                if manifest_path.exists() {
                    match self.load_manifest(&manifest_path).await {
                        Ok(manifest) => manifests.push(manifest),
                        Err(e) => warn!("Failed to load plugin manifest from {:?}: {}", manifest_path, e),
                    }
                }
            }
        }

        Ok(manifests)
    }

    async fn load_manifest(&self, path: &PathBuf) -> Result<PluginManifest> {
        let content = tokio::fs::read_to_string(path).await?;
        let manifest: PluginManifest = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Get or start a plugin
    pub async fn get_plugin(&self, spec: &PluginSpec) -> Result<Arc<Mutex<PluginInstance>>> {
        let mut plugins = self.plugins.lock().await;

        if let Some(instance) = plugins.get(&spec.name) {
            return Ok(Arc::clone(instance));
        }

        // Need to start the plugin
        let manifest = self.resolve_plugin(spec).await?;
        let plugin_path = self.plugin_dir.join(&spec.name).join(&manifest.build.command);

        let transport = StdioTransport::spawn(plugin_path.to_str().unwrap(), &[]).await
            .context(format!("Failed to start plugin: {}", spec.name))?;

        let instance = PluginInstance {
            manifest,
            transport: Some(transport),
            initialized: false,
        };

        let arc_instance = Arc::new(Mutex::new(instance));
        plugins.insert(spec.name.clone(), Arc::clone(&arc_instance));

        Ok(arc_instance)
    }

    async fn resolve_plugin(&self, spec: &PluginSpec) -> Result<PluginManifest> {
        match &spec.source {
            PluginSource::Local { path } => {
                let manifest_path = path.join("polyglot-plugin.toml");
                self.load_manifest(&manifest_path).await
            }
            PluginSource::Registry { name, version: _ } => {
                let manifest_path = self.plugin_dir.join(name).join("polyglot-plugin.toml");
                self.load_manifest(&manifest_path).await
            }
            PluginSource::Git { url: _, rev: _ } => {
                // TODO: Implement git plugin fetching
                anyhow::bail!("Git plugin source not yet implemented")
            }
        }
    }

    /// Initialize a plugin
    pub async fn initialize_plugin(
        &self,
        instance: Arc<Mutex<PluginInstance>>,
        _target_triple: &Triple,
        capabilities: PluginCapabilities,
    ) -> Result<()> {
        let mut instance = instance.lock().await;

        if instance.initialized {
            return Ok(());
        }

        let transport = instance.transport.as_mut()
            .context("Plugin transport not available")?;

        let _result = transport.initialize(capabilities).await
            .context("Failed to initialize plugin")?;

        instance.initialized = true;
        info!("Plugin initialized successfully");
        Ok(())
    }

    /// Compile a source file using a plugin
    pub async fn compile(
        &self,
        instance: Arc<Mutex<PluginInstance>>,
        source_path: &PathBuf,
        args: &[String],
        target_triple: &Triple,
        opt_level: polyglot_protocol::types::OptLevel,
    ) -> Result<CompileResult> {
        // Check initialization first without holding the lock for the whole operation
        {
            let instance = instance.lock().await;
            if !instance.initialized {
                anyhow::bail!("Plugin not initialized");
            }
        }

        // Now do the compilation
        let mut instance = instance.lock().await;
        let transport = instance.transport.as_mut()
            .context("Plugin transport not available")?;

        let params = CompileParams {
            source_path: source_path.to_string_lossy().to_string(),
            source_content: None,
            args: args.to_vec(),
            target_triple: target_triple.to_string(),
            opt_level,
            diagnostics: true,
            cache_key: None,
        };

        let result = transport.compile(params).await
            .context("Plugin compilation failed")?;

        Ok(result)
    }

    /// Shutdown all plugins
    pub async fn shutdown_all(&self) -> Result<()> {
        let mut plugins = self.plugins.lock().await;

        for (name, instance) in plugins.iter_mut() {
            let mut instance = instance.lock().await;
            if let Some(transport) = instance.transport.take() {
                if let Err(e) = transport.shutdown().await {
                    warn!("Failed to shutdown plugin {}: {}", name, e);
                }
            }
        }

        Ok(())
    }
}