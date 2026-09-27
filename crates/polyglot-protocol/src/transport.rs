use crate::types::{Request, RequestId, Response};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, error, warn};

/// Transport for communicating with plugins via stdio
pub struct StdioTransport {
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<BufReader<ChildStdout>>>,
    pending: Arc<Mutex<HashMap<RequestId, oneshot::Sender<Response>>>>,
    next_id: Arc<Mutex<u64>>,
}

impl StdioTransport {
    /// Create a new stdio transport by spawning the plugin process
    pub async fn spawn(command: &str, args: &[&str]) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn plugin process")?;

        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stderr = child.stderr.take().context("Failed to get stderr")?;

        // Spawn stderr reader
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("[plugin stderr] {}", line);
            }
        });

        let transport = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
        };

        // Start response reader
        transport.start_response_reader().await;

        Ok(transport)
    }

    /// Start reading responses from stdout
    async fn start_response_reader(&self) {
        let stdout = self.stdout.clone();
        let pending = self.pending.clone();

        tokio::spawn(async move {
            let mut reader = stdout.lock().await;
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // EOF
                        debug!("Plugin stdout closed");
                        break;
                    }
                    Ok(_) => {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        debug!("Received: {}", line);

                        match serde_json::from_str::<Response>(line) {
                            Ok(response) => {
                                let id = match &response {
                                    Response::Success(r) => r.id.clone(),
                                    Response::Error(r) => r.id.clone(),
                                };

                                let mut pending = pending.lock().await;
                                if let Some(sender) = pending.remove(&id) {
                                    let _ = sender.send(response);
                                } else {
                                    warn!("Received response for unknown id: {:?}", id);
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse response: {} - {}", e, line);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error reading from plugin: {}", e);
                        break;
                    }
                }
            }

            // Notify all pending requests that the connection is closed
            let mut pending = pending.lock().await;
            for (_, sender) in pending.drain() {
                let _ = sender.send(Response::Error(crate::types::ErrorResponse {
                    jsonrpc: "2.0".to_string(),
                    id: RequestId::Null,
                    error: crate::types::RpcError {
                        code: crate::types::error_codes::INTERNAL_ERROR,
                        message: "Plugin process terminated".to_string(),
                        data: None,
                    },
                }));
            }
        });
    }

    /// Send a request and wait for response
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Response> {
        let id = {
            let mut next_id = self.next_id.lock().await;
            let id = RequestId::Number(*next_id);
            *next_id += 1;
            id
        };

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending.lock().await;
            pending.insert(id.clone(), tx);
        }

        let request = crate::types::make_request(id.clone(), method, params);
        let request_json =
            serde_json::to_string(&request).context("Failed to serialize request")?;

        debug!("Sending: {}", request_json);

        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(request_json.as_bytes())
                .await
                .context("Failed to write request")?;
            stdin
                .write_all(b"\n")
                .await
                .context("Failed to write newline")?;
            stdin.flush().await.context("Failed to flush stdin")?;
        }

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                // Channel closed
                anyhow::bail!("Response channel closed unexpectedly");
            }
            Err(_) => {
                // Timeout
                let mut pending = self.pending.lock().await;
                pending.remove(&id);
                anyhow::bail!("Request timed out");
            }
        }
    }

    /// Send a notification (no response expected)
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        let notification = crate::types::make_notification(method, params);
        let notification_json =
            serde_json::to_string(&notification).context("Failed to serialize notification")?;

        debug!("Sending notification: {}", notification_json);

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(notification_json.as_bytes())
            .await
            .context("Failed to write notification")?;
        stdin
            .write_all(b"\n")
            .await
            .context("Failed to write newline")?;
        stdin.flush().await.context("Failed to flush stdin")?;

        Ok(())
    }

    /// Initialize the plugin
    pub async fn initialize(
        &self,
        capabilities: crate::types::PluginCapabilities,
    ) -> Result<crate::types::InitializeResult> {
        let params = crate::types::InitializeParams {
            protocol_version: crate::types::PROTOCOL_VERSION.to_string(),
            capabilities,
        };

        let response = self
            .request(
                crate::types::methods::INITIALIZE,
                Some(serde_json::to_value(params)?),
            )
            .await?;

        match response {
            Response::Success(r) => {
                let result: crate::types::InitializeResult = serde_json::from_value(r.result)
                    .context("Failed to parse initialize result")?;
                Ok(result)
            }
            Response::Error(e) => {
                anyhow::bail!("Initialize failed: {}", e.error.message);
            }
        }
    }

    /// Compile a source file
    pub async fn compile(
        &self,
        params: crate::types::CompileParams,
    ) -> Result<crate::types::CompileResult> {
        let response = self
            .request(
                crate::types::methods::COMPILE,
                Some(serde_json::to_value(params)?),
            )
            .await?;

        match response {
            Response::Success(r) => {
                let result: crate::types::CompileResult =
                    serde_json::from_value(r.result).context("Failed to parse compile result")?;
                Ok(result)
            }
            Response::Error(e) => {
                anyhow::bail!("Compile failed: {}", e.error.message);
            }
        }
    }

    /// Shutdown the plugin
    pub async fn shutdown(&self) -> Result<()> {
        let response = self.request(crate::types::methods::SHUTDOWN, None).await?;

        match response {
            Response::Success(_) => {
                // Send exit notification
                self.notify(crate::types::methods::EXIT, None).await?;
                Ok(())
            }
            Response::Error(e) => {
                anyhow::bail!("Shutdown failed: {}", e.error.message);
            }
        }
    }
}

/// Server-side transport for plugins
pub struct ServerTransport {
    reader: BufReader<tokio::io::Stdin>,
    writer: tokio::io::Stdout,
}

impl Default for ServerTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerTransport {
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
        }
    }

    /// Read next request
    pub async fn read_request(&mut self) -> Result<Option<Request>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line).await {
            Ok(0) => Ok(None), // EOF
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    return Ok(None);
                }

                match serde_json::from_str::<Request>(line) {
                    Ok(req) => Ok(Some(req)),
                    Err(e) => {
                        error!("Failed to parse request: {} - {}", e, line);
                        // Send parse error response if it was a call
                        // For now just return error
                        anyhow::bail!("Parse error: {}", e);
                    }
                }
            }
            Err(e) => anyhow::bail!("Read error: {}", e),
        }
    }

    /// Send a response
    pub async fn send_response(&mut self, response: Response) -> Result<()> {
        let json = serde_json::to_string(&response).context("Failed to serialize response")?;
        debug!("Sending response: {}", json);
        self.writer
            .write_all(json.as_bytes())
            .await
            .context("Failed to write response")?;
        self.writer
            .write_all(b"\n")
            .await
            .context("Failed to write newline")?;
        self.writer
            .flush()
            .await
            .context("Failed to flush stdout")?;
        Ok(())
    }

    /// Send a success response
    pub async fn send_success(&mut self, id: RequestId, result: Value) -> Result<()> {
        self.send_response(crate::types::make_success(id, result))
            .await
    }

    /// Send an error response
    pub async fn send_error(
        &mut self,
        id: RequestId,
        code: i32,
        message: String,
        data: Option<Value>,
    ) -> Result<()> {
        self.send_response(crate::types::make_error(id, code, message, data))
            .await
    }
}
