//! Encrypted transport stream wrapper

use snow::TransportState;
use std::sync::Mutex;

/// Encrypted stream wrapper
pub struct NoiseStream<S> {
    inner: S,
    transport: Mutex<TransportState>,
}

impl<S> NoiseStream<S> {
    /// Create a new encrypted stream from a completed handshake
    pub fn new(inner: S, transport: TransportState) -> Self {
        Self {
            inner,
            transport: Mutex::new(transport),
        }
    }
}
