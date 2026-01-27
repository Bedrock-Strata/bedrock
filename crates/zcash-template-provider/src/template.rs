//! Template Provider - stub module (full implementation in Task 6)

use crate::error::Result;
use crate::rpc::ZebraRpc;

/// Template Provider placeholder
pub struct TemplateProvider {
    _rpc: ZebraRpc,
}

impl TemplateProvider {
    /// Create a new Template Provider (stub)
    pub fn new(_url: &str) -> Result<Self> {
        let rpc = ZebraRpc::new(_url, None, None)?;
        Ok(Self { _rpc: rpc })
    }
}
