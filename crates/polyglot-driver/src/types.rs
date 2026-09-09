use polyglot_protocol::types::{OptLevel, PluginCapabilities};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use target_lexicon::Triple;

/// Build configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(with = "triple_serde")]
    pub target: Triple,
    pub optimization: OptLevel,
    pub output: OutputType,
    pub plugins: Vec<PluginSpec>,
    pub cache_dir: Option<PathBuf>,
    pub incremental: bool,
}

mod triple_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use target_lexicon::Triple;

    pub fn serialize<S>(triple: &Triple, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        triple.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Triple, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Output type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
    Executable,
    Object,
    Assembly,
    LlvmIr,
    Mlir,
}

/// Plugin specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSpec {
    pub name: String,
    pub version: String,
    pub source: PluginSource,
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Plugin source
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginSource {
    Registry { name: String, version: String },
    Git { url: String, rev: Option<String> },
    Local { path: PathBuf },
}

/// Project configuration (polyglot.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectMetadata,
    #[serde(default)]
    pub target: TargetConfig,
    #[serde(default)]
    pub plugins: HashMap<String, String>,
    #[serde(default)]
    pub bins: Vec<BinConfig>,
}

/// Project metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// Target configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "option_triple_serde")]
    pub triple: Option<Triple>,
    #[serde(default = "default_opt_level")]
    pub opt_level: OptLevel,
}

mod option_triple_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use target_lexicon::Triple;

    pub fn serialize<S>(triple: &Option<Triple>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match triple {
            Some(t) => t.to_string().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Triple>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        match s {
            Some(s) => Ok(Some(s.parse().map_err(serde::de::Error::custom)?)),
            None => Ok(None),
        }
    }
}

fn default_opt_level() -> OptLevel {
    OptLevel::Default
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            triple: None,
            opt_level: OptLevel::Default,
        }
    }
}

/// Binary configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinConfig {
    pub name: String,
    pub source: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Plugin manifest (from polyglot-plugin.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginMetadata,
    pub protocol: ProtocolConfig,
    pub capabilities: PluginCapabilities,
    pub build: PluginBuildConfig,
}

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub version: String,
    #[serde(default = "default_transport")]
    pub transport: TransportType,
}

fn default_transport() -> TransportType {
    TransportType::Stdio
}

/// Transport type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TransportType {
    Stdio,
    UnixSocket,
}

/// Build configuration in plugin manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginBuildConfig {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point: Option<String>,
}

/// Compilation unit
#[derive(Debug, Clone)]
pub struct CompilationUnit {
    pub source_path: PathBuf,
    pub plugin_name: String,
    pub args: Vec<String>,
    pub target_triple: Triple,
    pub opt_level: OptLevel,
}

/// Build result
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub output_path: PathBuf,
    pub object_files: Vec<PathBuf>,
    pub mlir_modules: Vec<String>,
}