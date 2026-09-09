use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Request {
    /// Method call with params and id
    Call(CallRequest),
    /// Notification (no response expected)
    Notification(NotificationRequest),
}

/// JSON-RPC 2.0 method call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 notification (no id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    /// Successful response
    Success(SuccessResponse),
    /// Error response
    Error(ErrorResponse),
}

/// Successful JSON-RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    pub result: Value,
}

/// Error JSON-RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    pub error: RpcError,
}

/// JSON-RPC error object
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[error("JSON-RPC error {code}: {message}")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Request ID can be string, number, or null
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(u64),
    Null,
}

impl Default for RequestId {
    fn default() -> Self {
        RequestId::Null
    }
}

impl From<u64> for RequestId {
    fn from(n: u64) -> Self {
        RequestId::Number(n)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        RequestId::String(s.to_string())
    }
}

/// Standard JSON-RPC error codes
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const SERVER_ERROR_START: i32 = -32099;
    pub const SERVER_ERROR_END: i32 = -32000;
}

/// Plugin-specific error codes
pub mod plugin_error_codes {
    pub const COMPILATION_FAILED: i32 = -32001;
    pub const UNSUPPORTED_LANGUAGE: i32 = -32002;
    pub const INVALID_SOURCE: i32 = -32003;
    pub const PLUGIN_NOT_INITIALIZED: i32 = -32004;
    pub const DIALECT_NOT_SUPPORTED: i32 = -32005;
}

/// Initialize request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: PluginCapabilities,
}

/// Plugin capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// MLIR dialects this plugin can produce/consume
    pub mlir_dialects: Vec<String>,
    /// Source file extensions this plugin handles
    #[serde(default)]
    pub source_extensions: Vec<String>,
    /// Languages this plugin supports
    #[serde(default)]
    pub languages: Vec<String>,
    /// Whether plugin supports incremental compilation
    #[serde(default)]
    pub incremental: bool,
}

/// Initialize response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: PluginCapabilities,
    pub plugin_info: PluginInfo,
}

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// Compile request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileParams {
    /// Path to source file (or "-" for stdin)
    pub source_path: String,
    /// Source content (if provided directly)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_content: Option<String>,
    /// Compiler arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Target triple
    pub target_triple: String,
    /// Optimization level
    #[serde(default = "default_opt_level")]
    pub opt_level: OptLevel,
    /// Whether to output diagnostics
    #[serde(default = "default_true")]
    pub diagnostics: bool,
    /// Incremental compilation cache key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

fn default_opt_level() -> OptLevel {
    OptLevel::Default
}

fn default_true() -> bool {
    true
}

/// Optimization level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OptLevel {
    None,
    Less,
    Default,
    Aggressive,
    Size,
    SizeAggressive,
}

impl Default for OptLevel {
    fn default() -> Self {
        OptLevel::Default
    }
}

/// Compile response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    /// MLIR module in textual format
    pub mlir_module: String,
    /// Diagnostics (errors, warnings, notes)
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    /// Cache key for incremental compilation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
}

/// Diagnostic message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Message
    pub message: String,
    /// Source location
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Related information
    #[serde(default)]
    pub related: Vec<RelatedInformation>,
}

/// Diagnostic severity
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Source location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// File path
    pub file: String,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Length of the span
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
}

/// Related diagnostic information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedInformation {
    pub location: Location,
    pub message: String,
}

/// Shutdown request (no params)
pub type ShutdownParams = Value;

/// Shutdown response (empty)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownResult {}

/// Plugin manifest (polyglot-plugin.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginMetadata,
    pub protocol: ProtocolConfig,
    pub capabilities: PluginCapabilities,
    pub build: BuildConfig,
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

/// Build configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Command to run the plugin (binary name)
    pub command: String,
    /// Optional: library-based plugin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// Optional: entry point for library-based plugin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point: Option<String>,
}

/// Method names
pub mod methods {
    pub const INITIALIZE: &str = "initialize";
    pub const COMPILE: &str = "compile";
    pub const SHUTDOWN: &str = "shutdown";
    pub const EXIT: &str = "exit"; // Notification
}

/// Helper to create a request
pub fn make_request(id: RequestId, method: &str, params: Option<Value>) -> Request {
    Request::Call(CallRequest {
        jsonrpc: "2.0".to_string(),
        id,
        method: method.to_string(),
        params,
    })
}

/// Helper to create a notification
pub fn make_notification(method: &str, params: Option<Value>) -> Request {
    Request::Notification(NotificationRequest {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
    })
}

/// Helper to create a success response
pub fn make_success(id: RequestId, result: Value) -> Response {
    Response::Success(SuccessResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result,
    })
}

/// Helper to create an error response
pub fn make_error(id: RequestId, code: i32, message: String, data: Option<Value>) -> Response {
    Response::Error(ErrorResponse {
        jsonrpc: "2.0".to_string(),
        id,
        error: RpcError { code, message, data },
    })
}