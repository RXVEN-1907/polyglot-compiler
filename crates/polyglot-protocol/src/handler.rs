use crate::types::{
    CompileParams, CompileResult, InitializeParams, InitializeResult, PluginCapabilities,
    PluginInfo, Request, RequestId, Response, ShutdownResult,
};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

/// Trait for implementing a Polyglot plugin
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin metadata
    fn info(&self) -> PluginInfo;

    /// Get plugin capabilities
    fn capabilities(&self) -> PluginCapabilities;

    /// Initialize the plugin
    async fn initialize(&mut self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            protocol_version: crate::types::PROTOCOL_VERSION.to_string(),
            capabilities: self.capabilities(),
            plugin_info: self.info(),
        })
    }

    /// Compile a source file to MLIR
    async fn compile(&mut self, params: CompileParams) -> Result<CompileResult>;

    /// Shutdown the plugin
    async fn shutdown(&mut self) -> Result<ShutdownResult> {
        Ok(ShutdownResult {})
    }
}

/// Closure handling one RPC method; yields the response future.
type MethodHandler = Box<
    dyn Fn(Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response>> + Send>>
        + Send
        + Sync,
>;

/// Request handler for the plugin server
pub struct PluginServer<P: Plugin> {
    plugin: P,
    method_handlers: HashMap<String, MethodHandler>,
}

impl<P: Plugin> PluginServer<P> {
    pub fn new(plugin: P) -> Self {
        let mut server = Self {
            plugin,
            method_handlers: HashMap::new(),
        };

        server.register_default_handlers();
        server
    }

    fn register_default_handlers(&mut self) {
        use crate::types::methods;

        // Initialize handler
        self.method_handlers.insert(
            methods::INITIALIZE.to_string(),
            Box::new(|req| {
                Box::pin(async move {
                    // Deserialize (and thus validate) params; the value itself is
                    // unused because initialize is deferred to the server loop.
                    let _params: InitializeParams =
                        serde_json::from_value(req.params().unwrap_or(Value::Null))?;
                    // Note: We can't call plugin.initialize here because we don't have &mut self
                    // This will be handled by the server loop
                    Ok(crate::types::make_error(
                        req.id().clone(),
                        crate::types::error_codes::METHOD_NOT_FOUND,
                        "Initialize must be handled by server".to_string(),
                        None,
                    ))
                })
            }),
        );

        // Compile handler
        self.method_handlers.insert(
            methods::COMPILE.to_string(),
            Box::new(|req| {
                Box::pin(async move {
                    Ok(crate::types::make_error(
                        req.id().clone(),
                        crate::types::error_codes::METHOD_NOT_FOUND,
                        "Compile must be handled by server".to_string(),
                        None,
                    ))
                })
            }),
        );

        // Shutdown handler
        self.method_handlers.insert(
            methods::SHUTDOWN.to_string(),
            Box::new(|req| {
                Box::pin(async move {
                    Ok(crate::types::make_error(
                        req.id().clone(),
                        crate::types::error_codes::METHOD_NOT_FOUND,
                        "Shutdown must be handled by server".to_string(),
                        None,
                    ))
                })
            }),
        );
    }

    /// Handle a request
    pub async fn handle_request(&mut self, request: Request) -> Result<Response> {
        match request {
            Request::Call(call) => {
                let method = call.method.clone();
                let id = call.id.clone();

                match method.as_str() {
                    crate::types::methods::INITIALIZE => {
                        let params: InitializeParams =
                            serde_json::from_value(call.params.unwrap_or(Value::Null))?;
                        match self.plugin.initialize(params).await {
                            Ok(result) => Ok(Response::Success(crate::types::SuccessResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: serde_json::to_value(result)?,
                            })),
                            Err(e) => Ok(crate::types::make_error(
                                id,
                                crate::types::error_codes::INTERNAL_ERROR,
                                e.to_string(),
                                None,
                            )),
                        }
                    }
                    crate::types::methods::COMPILE => {
                        let params: CompileParams =
                            serde_json::from_value(call.params.unwrap_or(Value::Null))?;
                        match self.plugin.compile(params).await {
                            Ok(result) => Ok(Response::Success(crate::types::SuccessResponse {
                                jsonrpc: "2.0".to_string(),
                                id,
                                result: serde_json::to_value(result)?,
                            })),
                            Err(e) => Ok(crate::types::make_error(
                                id,
                                crate::types::plugin_error_codes::COMPILATION_FAILED,
                                e.to_string(),
                                None,
                            )),
                        }
                    }
                    crate::types::methods::SHUTDOWN => match self.plugin.shutdown().await {
                        Ok(result) => Ok(Response::Success(crate::types::SuccessResponse {
                            jsonrpc: "2.0".to_string(),
                            id,
                            result: serde_json::to_value(result)?,
                        })),
                        Err(e) => Ok(crate::types::make_error(
                            id,
                            crate::types::error_codes::INTERNAL_ERROR,
                            e.to_string(),
                            None,
                        )),
                    },
                    _ => Ok(crate::types::make_error(
                        id,
                        crate::types::error_codes::METHOD_NOT_FOUND,
                        format!("Method not found: {}", method),
                        None,
                    )),
                }
            }
            Request::Notification(notif) => {
                if notif.method == crate::types::methods::EXIT {
                    // Exit notification - server should shut down
                    Ok(crate::types::make_error(
                        RequestId::Null,
                        crate::types::error_codes::INVALID_REQUEST,
                        "Exit notification received".to_string(),
                        None,
                    ))
                } else {
                    Ok(crate::types::make_error(
                        RequestId::Null,
                        crate::types::error_codes::METHOD_NOT_FOUND,
                        format!("Notification method not found: {}", notif.method),
                        None,
                    ))
                }
            }
        }
    }
}

/// Helper trait to extract id and params from Request
trait RequestExt {
    fn id(&self) -> RequestId;
    fn params(&self) -> Option<Value>;
}

impl RequestExt for Request {
    fn id(&self) -> RequestId {
        match self {
            Request::Call(call) => call.id.clone(),
            Request::Notification(_) => RequestId::Null,
        }
    }

    fn params(&self) -> Option<Value> {
        match self {
            Request::Call(call) => call.params.clone(),
            Request::Notification(notif) => notif.params.clone(),
        }
    }
}
