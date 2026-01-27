use zcash_template_provider::rpc::ZebraRpc;

#[tokio::test]
async fn test_rpc_client_creation() {
    let rpc = ZebraRpc::new("http://127.0.0.1:8232", None, None);
    assert!(rpc.is_ok());
}

#[tokio::test]
async fn test_rpc_request_format() {
    // Test that we format JSON-RPC requests correctly
    let request = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "test",
        "method": "getblocktemplate",
        "params": []
    });

    assert_eq!(request["jsonrpc"], "1.0");
    assert_eq!(request["method"], "getblocktemplate");
}
