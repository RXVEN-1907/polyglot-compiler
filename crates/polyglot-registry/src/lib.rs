//! Polyglot Registry - Plugin registry client

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Registry client
pub struct RegistryClient {
    base_url: String,
    client: reqwest::Client,
}

/// Plugin entry in registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPlugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub downloads: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Plugin version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginVersion {
    pub version: String,
    pub yanked: bool,
    pub dependencies: Vec<String>,
}

/// Search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub plugins: Vec<RegistryPlugin>,
    pub total: u64,
}

impl RegistryClient {
    /// Create a new registry client
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "https://plugins.polyglot.dev".to_string()),
            client: reqwest::Client::new(),
        }
    }

    /// Search for plugins
    pub async fn search(&self, query: &str) -> Result<SearchResults> {
        let url = format!("{}/api/v1/search?q={}", self.base_url, query);
        let response = self.client.get(&url).send().await?;
        let results = response.json::<SearchResults>().await?;
        Ok(results)
    }

    /// Get plugin metadata
    pub async fn get_plugin(&self, name: &str) -> Result<RegistryPlugin> {
        let url = format!("{}/api/v1/plugins/{}", self.base_url, name);
        let response = self.client.get(&url).send().await?;
        let plugin = response.json::<RegistryPlugin>().await?;
        Ok(plugin)
    }

    /// Get plugin versions
    pub async fn get_versions(&self, name: &str) -> Result<Vec<PluginVersion>> {
        let url = format!("{}/api/v1/plugins/{}/versions", self.base_url, name);
        let response = self.client.get(&url).send().await?;
        let versions = response.json::<Vec<PluginVersion>>().await?;
        Ok(versions)
    }

    /// Download plugin
    pub async fn download_plugin(
        &self,
        name: &str,
        version: &str,
        dest_dir: &PathBuf,
    ) -> Result<()> {
        let url = format!(
            "{}/api/v1/plugins/{}/{}/download",
            self.base_url, name, version
        );
        let response = self.client.get(&url).send().await?;
        let bytes = response.bytes().await?;

        // Extract tarball
        let tar = std::io::Cursor::new(bytes);
        let mut archive = tar::Archive::new(tar);
        archive
            .unpack(dest_dir)
            .context("Failed to extract plugin")?;

        Ok(())
    }

    /// Publish plugin (requires auth)
    pub async fn publish_plugin(
        &self,
        _manifest_path: &PathBuf,
        _package_path: &PathBuf,
        _token: &str,
    ) -> Result<()> {
        // TODO: Implement plugin publishing
        anyhow::bail!("Plugin publishing not yet implemented");
    }
}
