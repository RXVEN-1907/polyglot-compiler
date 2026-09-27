//! Integration tests for the driver/plugin protocol
//! (`polyglot-protocol`): serde roundtrips for params/results, JSON-RPC
//! frame shapes, the `PluginServer` dispatch table, and end-to-end
//! frame encode/decode through `StdioTransport`.

use polyglot_protocol::{
    error_codes, make_error, make_notification, make_request, make_success, methods,
    plugin_error_codes, CompileParams, CompileResult, Diagnostic as ProtocolDiagnostic,
    DiagnosticSeverity, ErrorResponse, InitializeParams, Location, OptLevel, Plugin,
    PluginCapabilities, PluginInfo, PluginServer, RelatedInformation, Request, RequestId, Response,
    RpcError, StdioTransport, SuccessResponse, PROTOCOL_VERSION,
};
use serde_json::{json, Value};

// ---- CompileParams / CompileResult ----------------------------------------

fn full_params() -> CompileParams {
    CompileParams {
        source_path: "hello.c".to_string(),
        source_content: Some("int main() { return 0; }".to_string()),
        args: vec!["-O2".to_string(), "-g".to_string()],
        target_triple: "x86_64-unknown-linux-gnu".to_string(),
        opt_level: OptLevel::Aggressive,
        diagnostics: false,
        cache_key: Some("abc123".to_string()),
    }
}

#[test]
fn compile_params_serde_roundtrip() {
    let params = full_params();
    let text = serde_json::to_string(&params).expect("serialize");
    let back: CompileParams = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&params).unwrap(),
        "roundtrip preserves every field"
    );
    // The wire format is what the protocol says it is.
    let wire: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(wire["source_path"], "hello.c");
    assert_eq!(wire["opt_level"], "aggressive");
    assert_eq!(wire["diagnostics"], false);
    assert_eq!(wire["cache_key"], "abc123");
}

#[test]
fn compile_params_defaults_apply() {
    // Only the two required fields; everything else takes its default.
    let params: CompileParams = serde_json::from_value(json!({
        "source_path": "t.c",
        "target_triple": "x86_64-unknown-linux-gnu",
    }))
    .expect("minimal params deserialize");
    assert_eq!(
        params.opt_level,
        OptLevel::Default,
        "opt_level defaults to default"
    );
    assert!(params.diagnostics, "diagnostics default on");
    assert!(params.args.is_empty(), "args default empty");
    assert_eq!(params.source_content, None);
    assert_eq!(params.cache_key, None);
}

#[test]
fn opt_level_parses_all_variants() {
    for (text, expected) in [
        ("none", OptLevel::None),
        ("less", OptLevel::Less),
        ("default", OptLevel::Default),
        ("aggressive", OptLevel::Aggressive),
        ("size", OptLevel::Size),
        ("sizeaggressive", OptLevel::SizeAggressive),
    ] {
        let parsed: OptLevel = serde_json::from_str(&format!("\"{text}\""))
            .unwrap_or_else(|e| panic!("`{text}` should parse: {e}"));
        assert_eq!(parsed, expected);
        let encoded = serde_json::to_string(&expected).unwrap();
        assert_eq!(encoded, format!("\"{text}\""), "serialization is symmetric");
    }
    assert!(
        serde_json::from_str::<OptLevel>("\"bogus\"").is_err(),
        "unknown opt levels are rejected"
    );
}

#[test]
fn compile_result_serde_roundtrip() {
    let result = CompileResult {
        mlir_module: "module @hello {}".to_string(),
        diagnostics: vec![ProtocolDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: "implicit declaration of `printf`".to_string(),
            location: Some(Location {
                file: "hello.c".to_string(),
                line: 3,
                column: 5,
                length: Some(6),
            }),
            related: vec![RelatedInformation {
                location: Location {
                    file: "hello.c".to_string(),
                    line: 3,
                    column: 5,
                    length: None,
                },
                message: "declared implicitly here".to_string(),
            }],
        }],
        cache_key: Some("deadbeef".to_string()),
    };
    let text = serde_json::to_string(&result).unwrap();
    let back: CompileResult = serde_json::from_str(&text).unwrap();
    assert_eq!(back.mlir_module, result.mlir_module);
    assert_eq!(back.cache_key, result.cache_key);
    assert_eq!(back.diagnostics.len(), 1);
    assert_eq!(back.diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(back.diagnostics[0].location.as_ref().unwrap().line, 3);

    // An empty diagnostics array is optional on the wire.
    let minimal: CompileResult =
        serde_json::from_value(json!({ "mlir_module": "module @x {}" })).unwrap();
    assert!(minimal.diagnostics.is_empty());
    assert_eq!(minimal.cache_key, None);
}

// ---- JSON-RPC frames --------------------------------------------------------

#[test]
fn request_frames_roundtrip() {
    let req = make_request(
        RequestId::Number(11),
        methods::COMPILE,
        Some(json!({"k": 1})),
    );
    let wire = serde_json::to_string(&req).unwrap();
    let v: Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 11);
    assert_eq!(v["method"], "compile");
    let back: Request = serde_json::from_str(&wire).unwrap();
    match back {
        Request::Call(c) => {
            assert_eq!(c.id, RequestId::Number(11));
            assert_eq!(c.method, "compile");
        }
        Request::Notification(_) => panic!("a call must not decode as a notification"),
    }

    // Notifications carry no id field on the wire.
    let notif = make_notification(methods::EXIT, None);
    let nv = serde_json::to_value(&notif).unwrap();
    assert!(nv.get("id").is_none(), "notification has no id: {nv}");
    assert_eq!(nv["method"], "exit");
    let back: Request = serde_json::from_str(&serde_json::to_string(&notif).unwrap()).unwrap();
    match back {
        Request::Notification(n) => assert_eq!(n.method, "exit"),
        Request::Call(_) => panic!("a notification must not decode as a call"),
    }
}

#[test]
fn response_helpers_roundtrip() {
    let success = make_success(RequestId::from("req-1"), json!({"ok": true}));
    let sv: Value = serde_json::to_value(&success).unwrap();
    assert_eq!(sv["jsonrpc"], "2.0");
    assert_eq!(sv["id"], "req-1");
    let back: Response = serde_json::from_value(sv).unwrap();
    match back {
        Response::Success(SuccessResponse { result, .. }) => assert_eq!(result["ok"], true),
        Response::Error(_) => panic!("success must decode as success"),
    }

    let err = make_error(
        RequestId::Number(3),
        plugin_error_codes::COMPILATION_FAILED,
        "boom".to_string(),
        Some(json!({"detail": "segfault"})),
    );
    let back: Response = serde_json::from_value(serde_json::to_value(&err).unwrap()).unwrap();
    match back {
        Response::Error(ErrorResponse {
            error:
                RpcError {
                    code,
                    message,
                    data,
                },
            ..
        }) => {
            assert_eq!(code, -32001);
            assert_eq!(message, "boom");
            assert_eq!(data.unwrap()["detail"], "segfault");
        }
        Response::Success(_) => panic!("error must decode as error"),
    }

    // `data` is omitted when None.
    let plain = make_error(
        RequestId::Null,
        error_codes::PARSE_ERROR,
        "bad json".into(),
        None,
    );
    let pv = serde_json::to_value(&plain).unwrap();
    assert!(pv["error"].get("data").is_none());
}

#[test]
fn request_id_roundtrips_all_shapes() {
    assert_eq!(
        serde_json::from_str::<RequestId>("\"str-id\"").unwrap(),
        RequestId::String("str-id".into())
    );
    assert_eq!(
        serde_json::from_str::<RequestId>("42").unwrap(),
        RequestId::Number(42)
    );
    assert_eq!(
        serde_json::from_str::<RequestId>("null").unwrap(),
        RequestId::Null
    );
    assert_eq!(
        serde_json::to_value(RequestId::Number(9)).unwrap(),
        json!(9)
    );
    assert_eq!(serde_json::to_value(&RequestId::Null).unwrap(), json!(null));
    // From conversions used by transports.
    assert_eq!(RequestId::from(7u64), RequestId::Number(7));
    assert_eq!(RequestId::from("s"), RequestId::String("s".into()));
}

// ---- PluginServer dispatch ---------------------------------------------------

struct MockPlugin;

#[async_trait::async_trait]
impl Plugin for MockPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: "mock".to_string(),
            version: "0.1.0".to_string(),
            description: "test double".to_string(),
            author: "tests".to_string(),
        }
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            mlir_dialects: vec!["mock".to_string()],
            source_extensions: vec!["c".to_string()],
            languages: vec!["c".to_string()],
            incremental: false,
        }
    }

    async fn compile(&mut self, params: CompileParams) -> anyhow::Result<CompileResult> {
        if params.source_path.contains("bad") {
            anyhow::bail!("invalid source: {}", params.source_path);
        }
        Ok(CompileResult {
            mlir_module: "module @mock {}".to_string(),
            diagnostics: vec![],
            cache_key: params.cache_key,
        })
    }
}

#[tokio::test]
async fn plugin_server_initializes_and_shuts_down() {
    let mut server = PluginServer::new(MockPlugin);

    let init = InitializeParams {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: PluginCapabilities {
            mlir_dialects: vec![],
            source_extensions: vec![],
            languages: vec![],
            incremental: false,
        },
    };
    let resp = server
        .handle_request(make_request(
            RequestId::Number(1),
            methods::INITIALIZE,
            Some(serde_json::to_value(&init).unwrap()),
        ))
        .await
        .expect("initialize handled");
    match resp {
        Response::Success(s) => {
            let result: polyglot_protocol::InitializeResult =
                serde_json::from_value(s.result).unwrap();
            assert_eq!(result.protocol_version, PROTOCOL_VERSION);
            assert_eq!(result.plugin_info.name, "mock");
            assert_eq!(result.capabilities.mlir_dialects, vec!["mock".to_string()]);
        }
        Response::Error(e) => panic!("initialize failed: {e:?}"),
    }

    let resp = server
        .handle_request(make_request(RequestId::Number(2), methods::SHUTDOWN, None))
        .await
        .expect("shutdown handled");
    assert!(matches!(resp, Response::Success(_)));
}

#[tokio::test]
async fn plugin_server_dispatches_compile() {
    let mut server = PluginServer::new(MockPlugin);
    let params = full_params();
    let resp = server
        .handle_request(make_request(
            RequestId::Number(7),
            methods::COMPILE,
            Some(serde_json::to_value(&params).unwrap()),
        ))
        .await
        .expect("compile handled");
    match resp {
        Response::Success(s) => {
            assert_eq!(
                s.id,
                RequestId::Number(7),
                "the response echoes the request id"
            );
            let result: CompileResult = serde_json::from_value(s.result).unwrap();
            assert_eq!(result.mlir_module, "module @mock {}");
            assert_eq!(result.cache_key, Some("abc123".to_string()));
        }
        Response::Error(e) => panic!("compile failed unexpectedly: {e:?}"),
    }
}

#[tokio::test]
async fn plugin_server_reports_compilation_failure_with_plugin_code() {
    let mut server = PluginServer::new(MockPlugin);
    let mut params = full_params();
    params.source_path = "bad.c".to_string();
    let resp = server
        .handle_request(make_request(
            RequestId::Number(8),
            methods::COMPILE,
            Some(serde_json::to_value(&params).unwrap()),
        ))
        .await
        .expect("compile handled");
    match resp {
        Response::Error(e) => {
            assert_eq!(e.error.code, plugin_error_codes::COMPILATION_FAILED);
            assert!(e.error.message.contains("invalid source"));
        }
        Response::Success(_) => panic!("a failing compile must answer with an error frame"),
    }
}

#[tokio::test]
async fn plugin_server_unknown_method_yields_method_not_found() {
    let mut server = PluginServer::new(MockPlugin);
    let resp = server
        .handle_request(make_request(RequestId::Number(9), "teleport", None))
        .await
        .expect("handled");
    match resp {
        Response::Error(e) => {
            assert_eq!(e.error.code, error_codes::METHOD_NOT_FOUND);
            assert!(e.error.message.contains("teleport"));
        }
        Response::Success(_) => panic!("unknown methods must be errors"),
    }
}

// ---- StdioTransport frame encode/decode in-process ---------------------------

/// A fake plugin: reads newline-delimited requests and always answers
/// with one fixed success frame for id 1 (the transport's first id).
#[tokio::test]
async fn stdio_transport_encodes_and_decodes_success_frames() {
    let script =
        r#"while read line; do echo '{"jsonrpc":"2.0","id":1,"result":{"pong":true}}'; done"#;
    let transport = StdioTransport::spawn("sh", &["-c", script])
        .await
        .expect("spawn fake plugin");

    let resp = transport
        .request("ping", Some(json!({"n": 1})))
        .await
        .expect("request answered");
    match resp {
        Response::Success(s) => {
            assert_eq!(s.id, RequestId::Number(1), "first request takes id 1");
            assert_eq!(s.jsonrpc, "2.0");
            assert_eq!(s.result["pong"], json!(true));
        }
        Response::Error(e) => panic!("unexpected error frame: {e:?}"),
    }

    // Notifications are one-way frames: encode + write succeed.
    transport
        .notify(methods::EXIT, None)
        .await
        .expect("notification written");
}

#[tokio::test]
async fn stdio_transport_decodes_error_frames() {
    let script = r#"while read line; do echo '{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"boom"}}'; done"#;
    let transport = StdioTransport::spawn("sh", &["-c", script])
        .await
        .expect("spawn fake plugin");

    let resp = transport.request("compile", None).await.expect("answered");
    match resp {
        Response::Error(e) => {
            assert_eq!(e.error.code, plugin_error_codes::COMPILATION_FAILED);
            assert_eq!(e.error.message, "boom");
        }
        Response::Success(_) => panic!("expected an error frame"),
    }
}
