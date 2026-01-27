//! Zebra JSON-RPC client

use crate::error::{Error, Result};
use crate::types::GetBlockTemplateResponse;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Zebra RPC client
pub struct ZebraRpc {
    client: Client,
    url: String,
    request_id: AtomicU64,
}

impl ZebraRpc {
    /// Create a new RPC client
    pub fn new(url: &str, _user: Option<&str>, _pass: Option<&str>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            url: url.to_string(),
            request_id: AtomicU64::new(1),
        })
    }

    /// Make a JSON-RPC request
    async fn request<T: DeserializeOwned, P: Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<T> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "1.0",
            "id": id.to_string(),
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        let body: Value = response.json().await?;

        if let Some(error) = body.get("error") {
            if !error.is_null() {
                return Err(Error::Rpc(error.to_string()));
            }
        }

        let result = body
            .get("result")
            .ok_or_else(|| Error::Rpc("missing result field".into()))?;

        serde_json::from_value(result.clone()).map_err(Error::Json)
    }

    /// Get a block template from Zebra
    pub async fn get_block_template(&self) -> Result<GetBlockTemplateResponse> {
        self.request("getblocktemplate", serde_json::json!([])).await
    }

    /// Submit a solved block to Zebra
    pub async fn submit_block(&self, block_hex: &str) -> Result<Option<String>> {
        self.request("submitblock", vec![block_hex]).await
    }

    /// Get the best block hash
    pub async fn get_best_block_hash(&self) -> Result<String> {
        self.request("getbestblockhash", serde_json::json!([])).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let rpc = ZebraRpc::new("http://127.0.0.1:8232", None, None);
        assert!(rpc.is_ok());
    }
}
