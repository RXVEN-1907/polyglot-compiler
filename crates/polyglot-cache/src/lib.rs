//! Polyglot Cache - Incremental compilation cache

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Compilation cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Hash of source + args + target + plugin version
    pub key: String,
    /// Path to cached object file
    pub object_path: PathBuf,
    /// MLIR module text (for debugging)
    pub mlir_module: String,
    /// Timestamp
    pub timestamp: u64,
}

/// Incremental compilation cache
pub struct CompilationCache {
    cache_dir: PathBuf,
    index: HashMap<String, CacheEntry>,
}

impl CompilationCache {
    /// Create a new cache
    pub async fn new(cache_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&cache_dir).await?;
        let index_path = cache_dir.join("index.json");

        let index = if index_path.exists() {
            let content = fs::read_to_string(&index_path).await?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { cache_dir, index })
    }

    /// Generate cache key
    pub fn generate_key(
        source_content: &str,
        plugin_name: &str,
        plugin_version: &str,
        args: &[String],
        target_triple: &str,
        opt_level: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(source_content.as_bytes());
        hasher.update(plugin_name.as_bytes());
        hasher.update(plugin_version.as_bytes());
        for arg in args {
            hasher.update(arg.as_bytes());
        }
        hasher.update(target_triple.as_bytes());
        hasher.update(opt_level.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get cached entry if exists
    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.index.get(key)
    }

    /// Store entry in cache
    pub async fn put(&mut self, key: String, entry: CacheEntry) -> Result<()> {
        self.index.insert(key.clone(), entry);
        self.save_index().await
    }

    /// Save index to disk
    async fn save_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.index)?;
        fs::write(index_path, content).await?;
        Ok(())
    }

    /// Get object file path for key
    pub fn object_path(&self, key: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.o", key))
    }

    /// Clean old entries (keep last N)
    pub async fn clean(&mut self, keep: usize) -> Result<()> {
        if self.index.len() <= keep {
            return Ok(());
        }

        // Sort by timestamp
        let mut entries: Vec<_> = self.index.drain().collect();
        entries.sort_by_key(|(_, e)| e.timestamp);

        // Remove oldest
        let to_remove = entries.len() - keep;
        for (key, entry) in entries.drain(..to_remove) {
            // Remove object file
            let _ = fs::remove_file(&entry.object_path).await;
        }

        // Keep remaining
        self.index = entries.into_iter().collect();
        self.save_index().await
    }
}