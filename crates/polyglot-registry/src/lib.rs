//! Polyglot Registry - Plugin registry client
//!
//! Talks to a plugin registry over a small HTTP contract:
//!
//! * `GET  {base}/api/v1/...` for search / metadata / downloads
//! * `PUT  {base}/api/plugins/{name}` to publish a plugin package
//!   (manifest + sources archived as an uncompressed tar, authenticated
//!   with a bearer token)
//!
//! It also parses registry index files (`registry/index.json`: a JSON
//! object with a `plugins` array), the same format the `polyglot plugin`
//! command consumes.

use anyhow::{bail, Context, Result};
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

/// One entry of a registry index file (`registry/index.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Registry name (e.g. "clang").
    pub name: String,
    /// Plugin binary name (e.g. "polyglot-clang"); defaults to
    /// "polyglot-{name}" when absent.
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Git repository to clone the plugin from.
    #[serde(default)]
    pub repository: Option<String>,
    /// Subdirectory of the repository holding the plugin crate.
    #[serde(default)]
    pub path: Option<String>,
}

impl RegistryEntry {
    /// The plugin's binary name, defaulting to "polyglot-{name}".
    pub fn binary_name(&self) -> String {
        self.plugin
            .clone()
            .unwrap_or_else(|| format!("polyglot-{}", self.name))
    }
}

/// A registry index file: `{"version": 1, "plugins": [...]}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    #[serde(default = "default_index_version")]
    pub version: u32,
    #[serde(default)]
    pub plugins: Vec<RegistryEntry>,
}

fn default_index_version() -> u32 {
    1
}

/// Parse a registry index (JSON text) into a [`RegistryIndex`].
pub fn parse_index(json: &str) -> Result<RegistryIndex> {
    let index: RegistryIndex = serde_json::from_str(json).context("invalid registry index JSON")?;
    Ok(index)
}

/// Build the publish endpoint URL for a plugin:
/// `{base}/api/plugins/{name}` (the PUT target of [`RegistryClient::publish_plugin`]).
pub fn publish_url(base_url: &str, name: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{}/api/plugins/{}", base, name)
}

/// Extract `(name, version)` from a polyglot-plugin.toml manifest
/// (the `[plugin]` table).
pub fn manifest_identity(manifest: &str) -> Result<(String, String)> {
    let value: toml::Value = toml::from_str(manifest).context("invalid polyglot-plugin.toml")?;
    let plugin = value
        .get("plugin")
        .context("manifest is missing the [plugin] table")?;
    let name = plugin
        .get("name")
        .and_then(|v| v.as_str())
        .context("manifest is missing plugin.name")?
        .to_string();
    let version = plugin
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    Ok((name, version))
}

/// Archive a plugin package directory (recursively) into an uncompressed
/// tar, the publish request body.
fn tar_directory(dir: &PathBuf) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    builder
        .append_dir_all(".", dir)
        .with_context(|| format!("failed to archive {}", dir.display()))?;
    builder
        .into_inner()
        .context("failed to finish the tar archive")
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

    /// Publish a plugin (requires auth): reads the plugin manifest for its
    /// identity, archives `package_path` as a tar, and PUTs it to
    /// `{base}/api/plugins/{name}` with the token as a bearer credential.
    /// Returns the server's response body (usually a JSON acknowledgement).
    pub async fn publish_plugin(
        &self,
        manifest_path: &PathBuf,
        package_path: &PathBuf,
        token: &str,
    ) -> Result<String> {
        let manifest = std::fs::read_to_string(manifest_path).with_context(|| {
            format!("failed to read plugin manifest {}", manifest_path.display())
        })?;
        let (name, version) = manifest_identity(&manifest)?;
        let url = publish_url(&self.base_url, &name);
        let body = tar_directory(package_path)?;

        let response = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("X-Plugin-Version", version)
            .header("Content-Type", "application/x-tar")
            .body(body)
            .send()
            .await
            .context("publish request failed")?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("publish failed: HTTP {}: {}", status, text.trim());
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::io::Write;

    const SAMPLE_INDEX: &str = r#"{
        "version": 1,
        "description": "Official polyglot plugin registry index",
        "plugins": [
            {
                "name": "clang",
                "plugin": "polyglot-clang",
                "version": "0.1.0",
                "description": "C/C++ frontend via Clang",
                "repository": "https://github.com/RXVEN-1907/polyglot-compiler",
                "path": "plugins/polyglot-clang"
            }
        ]
    }"#;

    #[test]
    fn parse_index_full() {
        let index = parse_index(SAMPLE_INDEX).expect("index should parse");
        assert_eq!(index.version, 1);
        assert_eq!(index.plugins.len(), 1);
        let entry = &index.plugins[0];
        assert_eq!(entry.name, "clang");
        assert_eq!(entry.binary_name(), "polyglot-clang");
        assert_eq!(entry.version.as_deref(), Some("0.1.0"));
        assert_eq!(
            entry.repository.as_deref(),
            Some("https://github.com/RXVEN-1907/polyglot-compiler")
        );
        assert_eq!(entry.path.as_deref(), Some("plugins/polyglot-clang"));
    }

    #[test]
    fn parse_index_minimal_defaults() {
        let index = parse_index("{}").expect("empty index should parse");
        assert_eq!(index.version, 1);
        assert!(index.plugins.is_empty());

        let index = parse_index(r#"{"plugins": [{"name": "demo"}]}"#)
            .expect("name-only entry should parse");
        assert_eq!(index.plugins[0].binary_name(), "polyglot-demo");
        assert_eq!(index.plugins[0].version, None);
    }

    #[test]
    fn parse_index_rejects_garbage() {
        assert!(parse_index("not json at all").is_err());
        assert!(parse_index(r#"{"plugins": "nope"}"#).is_err());
    }

    #[test]
    fn publish_url_building() {
        assert_eq!(
            publish_url("https://plugins.polyglot.dev", "clang"),
            "https://plugins.polyglot.dev/api/plugins/clang"
        );
        assert_eq!(
            publish_url("http://127.0.0.1:8080/", "polyglot-clang"),
            "http://127.0.0.1:8080/api/plugins/polyglot-clang"
        );
    }

    #[test]
    fn manifest_identity_extracts_plugin_table() {
        let (name, version) = manifest_identity(
            "[plugin]\nname = \"clang\"\nversion = \"0.1.0\"\n\
             [capabilities]\nlanguages = [\"c\"]\n",
        )
        .expect("manifest should parse");
        assert_eq!(name, "clang");
        assert_eq!(version, "0.1.0");
    }

    #[test]
    fn manifest_identity_rejects_bad_manifests() {
        assert!(manifest_identity("no toml: [[").is_err());
        assert!(manifest_identity("[capabilities]\nx = 1\n").is_err());
        // version is optional and defaults to 0.0.0
        let (_, version) = manifest_identity("[plugin]\nname = \"demo\"\n").expect("parses");
        assert_eq!(version, "0.0.0");
    }

    /// Serve one PUT on a local std TcpListener and check the publish
    /// contract end-to-end: method, path, auth header and body delivery.
    #[tokio::test]
    async fn publish_against_local_mock() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let n = stream.read(&mut chunk).expect("read head");
                buf.extend_from_slice(&chunk[..n]);
                if n == 0 || buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf).to_string();
            let content_length: usize = head
                .split("\r\n")
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse().ok())
                })
                .unwrap_or(0);
            let header_end = head.find("\r\n\r\n").map(|i| i + 4).unwrap_or(buf.len());
            let mut body_bytes = buf.len().saturating_sub(header_end);
            while body_bytes < content_length {
                let n = stream.read(&mut chunk).expect("read body");
                if n == 0 {
                    break;
                }
                body_bytes += n;
            }
            let request_line = head.lines().next().unwrap_or("").to_string();
            let lower = head.to_ascii_lowercase();
            let had_auth = lower.contains("authorization: bearer test-token");
            let had_version = lower.contains("x-plugin-version: 0.1.0");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                      Content-Length: 11\r\n\r\n{\"ok\":true}",
                )
                .expect("write response");
            (request_line, had_auth, had_version, body_bytes)
        });

        let tmp =
            std::env::temp_dir().join(format!("polyglot-registry-publish-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("mkdir");
        std::fs::write(
            tmp.join("polyglot-plugin.toml"),
            "[plugin]\nname = \"clang\"\nversion = \"0.1.0\"\n",
        )
        .expect("write manifest");
        std::fs::write(tmp.join("plugin.txt"), "payload\n").expect("write file");

        let client = RegistryClient::new(Some(format!("http://{}", addr)));
        let body = client
            .publish_plugin(&tmp.join("polyglot-plugin.toml"), &tmp, "test-token")
            .await
            .expect("publish should succeed");
        assert_eq!(body, "{\"ok\":true}");

        let (request_line, had_auth, had_version, body_bytes) =
            handle.join().expect("mock server thread");
        assert_eq!(request_line, "PUT /api/plugins/clang HTTP/1.1");
        assert!(had_auth, "expected the bearer token header");
        assert!(had_version, "expected the X-Plugin-Version header");
        assert!(body_bytes > 0, "expected a non-empty tar body");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
