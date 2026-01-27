# Stratum V2 Zcash Phase 5: Security & Production Hardening

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Noise Protocol encryption and full observability (metrics, structured logging, tracing) to prepare the Stratum V2 Zcash implementation for production deployment.

**Architecture:** New `zcash-stratum-noise` crate provides Noise NK handshake and encrypted transport wrappers. Pool Server, JD Server, and JD Client gain configurable encryption. Observability via Prometheus metrics endpoint, JSON structured logging, and OpenTelemetry tracing spans.

**Tech Stack:** Rust 1.75+, `snow` (Noise Protocol), `prometheus` (metrics), `tracing` + `tracing-subscriber` (logging/spans), `opentelemetry` + `opentelemetry-otlp` (distributed tracing)

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Encryption Protocol | Noise NK | SV2 spec standard, simpler than TLS |
| Authentication | Open (encryption only) | MVP - auth restrictions in future phase |
| Observability | Full (metrics + logs + traces) | Production-grade visibility |
| Multi-miner | Single-miner only | Keep JD Client simple |
| Encryption Scope | Configurable per connection | Flexibility for dev/prod |

---

## Crate Structure

```
crates/zcash-stratum-noise/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── keys.rs             # Keypair generation and storage
│   ├── handshake.rs        # Noise NK handshake (initiator/responder)
│   └── transport.rs        # NoiseStream wrapper for encrypted I/O
└── tests/
    └── integration_tests.rs
```

---

## Task 1: Initialize Noise Crate with Keypair Management

**Files:**
- Create: `crates/zcash-stratum-noise/Cargo.toml`
- Create: `crates/zcash-stratum-noise/src/lib.rs`
- Create: `crates/zcash-stratum-noise/src/keys.rs`

**Step 1: Create Cargo.toml**

Create `crates/zcash-stratum-noise/Cargo.toml`:

```toml
[package]
name = "zcash-stratum-noise"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Noise Protocol encryption for Zcash Stratum V2"

[dependencies]
snow = "0.9"
thiserror.workspace = true
tracing.workspace = true
rand = "0.8"
hex.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
```

**Step 2: Create lib.rs**

Create `crates/zcash-stratum-noise/src/lib.rs`:

```rust
//! Noise Protocol encryption for Zcash Stratum V2
//!
//! Implements the Noise NK handshake pattern as specified by SV2.
//! - Server has static keypair (known to clients)
//! - Client uses ephemeral keys
//!
//! ## Usage
//!
//! ```ignore
//! // Server side
//! let keypair = Keypair::generate();
//! let responder = NoiseResponder::new(&keypair);
//! let stream = responder.accept(tcp_stream).await?;
//!
//! // Client side
//! let initiator = NoiseInitiator::new(server_public_key);
//! let stream = initiator.connect(tcp_stream).await?;
//! ```

pub mod keys;
pub mod handshake;
pub mod transport;

pub use keys::{Keypair, PublicKey};
pub use handshake::{NoiseInitiator, NoiseResponder};
pub use transport::NoiseStream;

/// Noise protocol pattern used (NK = known server key)
pub const NOISE_PATTERN: &str = "Noise_NK_25519_ChaChaPoly_BLAKE2s";
```

**Step 3: Create keys.rs**

Create `crates/zcash-stratum-noise/src/keys.rs`:

```rust
//! Keypair generation and management for Noise Protocol

use snow::Keypair as SnowKeypair;
use std::fmt;
use thiserror::Error;

/// A 32-byte Curve25519 public key
#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, KeyError> {
        let bytes = hex::decode(hex).map_err(|_| KeyError::InvalidHex)?;
        if bytes.len() != 32 {
            return Err(KeyError::InvalidLength(bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({}...)", &self.to_hex()[..8])
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// A Curve25519 keypair (public + private)
pub struct Keypair {
    /// Public key (can be shared)
    pub public: PublicKey,
    /// Private key (keep secret)
    private: [u8; 32],
}

impl Keypair {
    /// Generate a new random keypair
    pub fn generate() -> Self {
        let builder = snow::Builder::new(crate::NOISE_PATTERN.parse().unwrap());
        let snow_keypair = builder.generate_keypair().unwrap();

        let mut public = [0u8; 32];
        let mut private = [0u8; 32];
        public.copy_from_slice(&snow_keypair.public);
        private.copy_from_slice(&snow_keypair.private);

        Self {
            public: PublicKey(public),
            private,
        }
    }

    /// Create from existing private key bytes
    pub fn from_private(private: [u8; 32]) -> Self {
        // Derive public key from private using snow
        let builder = snow::Builder::new(crate::NOISE_PATTERN.parse().unwrap());
        let snow_keypair = builder
            .local_private_key(&private)
            .generate_keypair()
            .unwrap();

        let mut public = [0u8; 32];
        public.copy_from_slice(&snow_keypair.public);

        Self {
            public: PublicKey(public),
            private,
        }
    }

    /// Load from hex-encoded private key
    pub fn from_private_hex(hex: &str) -> Result<Self, KeyError> {
        let bytes = hex::decode(hex).map_err(|_| KeyError::InvalidHex)?;
        if bytes.len() != 32 {
            return Err(KeyError::InvalidLength(bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self::from_private(arr))
    }

    /// Export private key as hex (for config storage)
    pub fn private_hex(&self) -> String {
        hex::encode(&self.private)
    }

    /// Get private key bytes (for snow)
    pub(crate) fn private_bytes(&self) -> &[u8; 32] {
        &self.private
    }
}

impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Keypair")
            .field("public", &self.public)
            .field("private", &"[redacted]")
            .finish()
    }
}

#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Invalid hex encoding")]
    InvalidHex,
    #[error("Invalid key length: expected 32, got {0}")]
    InvalidLength(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();

        // Different keypairs should have different public keys
        assert_ne!(kp1.public.0, kp2.public.0);
    }

    #[test]
    fn test_keypair_roundtrip() {
        let kp = Keypair::generate();
        let hex = kp.private_hex();
        let restored = Keypair::from_private_hex(&hex).unwrap();

        assert_eq!(kp.public.0, restored.public.0);
    }

    #[test]
    fn test_public_key_hex() {
        let kp = Keypair::generate();
        let hex = kp.public.to_hex();
        let restored = PublicKey::from_hex(&hex).unwrap();

        assert_eq!(kp.public.0, restored.0);
    }

    #[test]
    fn test_invalid_hex() {
        let result = PublicKey::from_hex("not-valid-hex");
        assert!(matches!(result, Err(KeyError::InvalidHex)));
    }

    #[test]
    fn test_invalid_length() {
        let result = PublicKey::from_hex("0102030405");
        assert!(matches!(result, Err(KeyError::InvalidLength(5))));
    }
}
```

**Step 4: Verify compilation**

Run: `cargo check -p zcash-stratum-noise`
Expected: PASS

**Step 5: Run tests**

Run: `cargo test -p zcash-stratum-noise`
Expected: All 5 tests pass

**Step 6: Commit**

```bash
git add crates/zcash-stratum-noise/
git commit -m "feat(noise): initialize crate with keypair management"
```

---

## Task 2: Implement Noise NK Handshake

**Files:**
- Create: `crates/zcash-stratum-noise/src/handshake.rs`

**Step 1: Create handshake.rs**

Create `crates/zcash-stratum-noise/src/handshake.rs`:

```rust
//! Noise NK handshake implementation
//!
//! NK pattern: Client knows server's static public key.
//! - Client (initiator): ephemeral key only
//! - Server (responder): static keypair

use crate::keys::{Keypair, PublicKey};
use crate::transport::NoiseStream;
use snow::{Builder, HandshakeState, TransportState};
use std::io;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, trace};

/// Maximum handshake message size
const MAX_HANDSHAKE_MSG: usize = 65535;

/// Noise handshake initiator (client side)
pub struct NoiseInitiator {
    server_public_key: PublicKey,
}

impl NoiseInitiator {
    /// Create initiator with known server public key
    pub fn new(server_public_key: PublicKey) -> Self {
        Self { server_public_key }
    }

    /// Perform handshake and return encrypted stream
    pub async fn connect(self, mut stream: TcpStream) -> Result<NoiseStream<TcpStream>, HandshakeError> {
        debug!("Starting Noise NK handshake as initiator");

        let builder: Builder<'_> = Builder::new(crate::NOISE_PATTERN.parse().unwrap());
        let mut handshake = builder
            .remote_public_key(self.server_public_key.as_bytes())
            .build_initiator()
            .map_err(HandshakeError::Snow)?;

        // -> e, es (client sends ephemeral, establishes shared secret)
        let mut msg = vec![0u8; MAX_HANDSHAKE_MSG];
        let len = handshake.write_message(&[], &mut msg).map_err(HandshakeError::Snow)?;
        trace!("Sending handshake message: {} bytes", len);

        stream.write_u16(len as u16).await?;
        stream.write_all(&msg[..len]).await?;

        // <- e, ee (server responds)
        let len = stream.read_u16().await? as usize;
        if len > MAX_HANDSHAKE_MSG {
            return Err(HandshakeError::MessageTooLarge(len));
        }
        let mut msg = vec![0u8; len];
        stream.read_exact(&mut msg).await?;
        trace!("Received handshake response: {} bytes", len);

        let mut payload = vec![0u8; MAX_HANDSHAKE_MSG];
        let _payload_len = handshake.read_message(&msg, &mut payload).map_err(HandshakeError::Snow)?;

        // Transition to transport mode
        let transport = handshake.into_transport_mode().map_err(HandshakeError::Snow)?;
        debug!("Noise handshake complete (initiator)");

        Ok(NoiseStream::new(stream, transport))
    }
}

/// Noise handshake responder (server side)
pub struct NoiseResponder {
    keypair: Keypair,
}

impl NoiseResponder {
    /// Create responder with server's static keypair
    pub fn new(keypair: Keypair) -> Self {
        Self { keypair }
    }

    /// Get the public key clients need to connect
    pub fn public_key(&self) -> &PublicKey {
        &self.keypair.public
    }

    /// Accept a connection and perform handshake
    pub async fn accept(&self, mut stream: TcpStream) -> Result<NoiseStream<TcpStream>, HandshakeError> {
        debug!("Starting Noise NK handshake as responder");

        let builder: Builder<'_> = Builder::new(crate::NOISE_PATTERN.parse().unwrap());
        let mut handshake = builder
            .local_private_key(self.keypair.private_bytes())
            .build_responder()
            .map_err(HandshakeError::Snow)?;

        // <- e, es (receive client's ephemeral)
        let len = stream.read_u16().await? as usize;
        if len > MAX_HANDSHAKE_MSG {
            return Err(HandshakeError::MessageTooLarge(len));
        }
        let mut msg = vec![0u8; len];
        stream.read_exact(&mut msg).await?;
        trace!("Received handshake message: {} bytes", len);

        let mut payload = vec![0u8; MAX_HANDSHAKE_MSG];
        let _payload_len = handshake.read_message(&msg, &mut payload).map_err(HandshakeError::Snow)?;

        // -> e, ee (send server's ephemeral)
        let mut response = vec![0u8; MAX_HANDSHAKE_MSG];
        let len = handshake.write_message(&[], &mut response).map_err(HandshakeError::Snow)?;
        trace!("Sending handshake response: {} bytes", len);

        stream.write_u16(len as u16).await?;
        stream.write_all(&response[..len]).await?;

        // Transition to transport mode
        let transport = handshake.into_transport_mode().map_err(HandshakeError::Snow)?;
        debug!("Noise handshake complete (responder)");

        Ok(NoiseStream::new(stream, transport))
    }
}

#[derive(Error, Debug)]
pub enum HandshakeError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Noise protocol error: {0}")]
    Snow(snow::Error),

    #[error("Handshake message too large: {0} bytes")]
    MessageTooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_handshake_roundtrip() {
        let server_keypair = Keypair::generate();
        let server_public = server_keypair.public.clone();

        // Start server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let responder = NoiseResponder::new(server_keypair);
            responder.accept(stream).await.unwrap()
        });

        // Connect client
        let client_stream = TcpStream::connect(addr).await.unwrap();
        let initiator = NoiseInitiator::new(server_public);
        let _client_noise = initiator.connect(client_stream).await.unwrap();

        // Server should complete too
        let _server_noise = server_handle.await.unwrap();
    }
}
```

**Step 2: Verify compilation**

Run: `cargo check -p zcash-stratum-noise`
Expected: PASS (will fail until transport.rs exists)

**Step 3: Commit (after Task 3)**

---

## Task 3: Implement Encrypted Transport Stream

**Files:**
- Create: `crates/zcash-stratum-noise/src/transport.rs`

**Step 1: Create transport.rs**

Create `crates/zcash-stratum-noise/src/transport.rs`:

```rust
//! Encrypted transport stream wrapper
//!
//! Wraps a TcpStream with Noise encryption, providing AsyncRead/AsyncWrite.

use snow::TransportState;
use std::io;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tracing::trace;

/// Maximum message size for Noise transport (64KB - overhead)
const MAX_MESSAGE_SIZE: usize = 65535 - 16; // 16 bytes for AEAD tag

/// Encrypted stream wrapper
pub struct NoiseStream<S> {
    inner: S,
    transport: Mutex<TransportState>,
    read_buffer: Vec<u8>,
    read_pos: usize,
}

impl<S> NoiseStream<S> {
    /// Create a new encrypted stream from a completed handshake
    pub fn new(inner: S, transport: TransportState) -> Self {
        Self {
            inner,
            transport: Mutex::new(transport),
            read_buffer: Vec::new(),
            read_pos: 0,
        }
    }

    /// Get reference to inner stream
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Get mutable reference to inner stream
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Consume and return inner stream (drops encryption state)
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl NoiseStream<TcpStream> {
    /// Read an encrypted message and decrypt it
    pub async fn read_message(&mut self) -> io::Result<Vec<u8>> {
        // Read length prefix
        let len = self.inner.read_u16().await? as usize;
        if len == 0 {
            return Ok(Vec::new());
        }

        // Read ciphertext
        let mut ciphertext = vec![0u8; len];
        self.inner.read_exact(&mut ciphertext).await?;

        // Decrypt
        let mut plaintext = vec![0u8; len];
        let transport = self.transport.lock().unwrap();
        let plaintext_len = transport
            .read_message(&ciphertext, &mut plaintext)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        plaintext.truncate(plaintext_len);
        trace!("Decrypted message: {} bytes", plaintext_len);
        Ok(plaintext)
    }

    /// Encrypt and write a message
    pub async fn write_message(&mut self, plaintext: &[u8]) -> io::Result<()> {
        if plaintext.len() > MAX_MESSAGE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Message too large: {} > {}", plaintext.len(), MAX_MESSAGE_SIZE),
            ));
        }

        // Encrypt
        let mut ciphertext = vec![0u8; plaintext.len() + 16]; // AEAD tag
        let ciphertext_len = {
            let mut transport = self.transport.lock().unwrap();
            transport
                .write_message(plaintext, &mut ciphertext)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
        };

        // Write with length prefix
        self.inner.write_u16(ciphertext_len as u16).await?;
        self.inner.write_all(&ciphertext[..ciphertext_len]).await?;
        trace!("Encrypted message: {} -> {} bytes", plaintext.len(), ciphertext_len);
        Ok(())
    }

    /// Flush the underlying stream
    pub async fn flush(&mut self) -> io::Result<()> {
        self.inner.flush().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::{NoiseInitiator, NoiseResponder};
    use crate::keys::Keypair;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_encrypted_communication() {
        let server_keypair = Keypair::generate();
        let server_public = server_keypair.public.clone();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Server task
        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let responder = NoiseResponder::new(server_keypair);
            let mut noise = responder.accept(stream).await.unwrap();

            // Receive message
            let msg = noise.read_message().await.unwrap();
            assert_eq!(msg, b"Hello from client!");

            // Send response
            noise.write_message(b"Hello from server!").await.unwrap();
        });

        // Client
        let client_stream = TcpStream::connect(addr).await.unwrap();
        let initiator = NoiseInitiator::new(server_public);
        let mut client_noise = initiator.connect(client_stream).await.unwrap();

        // Send message
        client_noise.write_message(b"Hello from client!").await.unwrap();

        // Receive response
        let response = client_noise.read_message().await.unwrap();
        assert_eq!(response, b"Hello from server!");

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_large_message() {
        let server_keypair = Keypair::generate();
        let server_public = server_keypair.public.clone();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let responder = NoiseResponder::new(server_keypair);
            let mut noise = responder.accept(stream).await.unwrap();

            let msg = noise.read_message().await.unwrap();
            assert_eq!(msg.len(), 10000);
            assert!(msg.iter().all(|&b| b == 0xAA));
        });

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let initiator = NoiseInitiator::new(server_public);
        let mut client_noise = initiator.connect(client_stream).await.unwrap();

        // Send 10KB message
        let large_msg = vec![0xAA; 10000];
        client_noise.write_message(&large_msg).await.unwrap();

        server_handle.await.unwrap();
    }
}
```

**Step 2: Verify compilation and tests**

Run: `cargo test -p zcash-stratum-noise`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/zcash-stratum-noise/
git commit -m "feat(noise): implement NK handshake and encrypted transport"
```

---

## Task 4: Add Observability Crate

**Files:**
- Create: `crates/zcash-stratum-observability/Cargo.toml`
- Create: `crates/zcash-stratum-observability/src/lib.rs`
- Create: `crates/zcash-stratum-observability/src/metrics.rs`
- Create: `crates/zcash-stratum-observability/src/logging.rs`
- Create: `crates/zcash-stratum-observability/src/tracing_setup.rs`

**Step 1: Create Cargo.toml**

Create `crates/zcash-stratum-observability/Cargo.toml`:

```toml
[package]
name = "zcash-stratum-observability"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Observability (metrics, logging, tracing) for Zcash Stratum V2"

[dependencies]
prometheus = "0.13"
tracing.workspace = true
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
opentelemetry = { version = "0.21", features = ["trace"] }
opentelemetry_sdk = { version = "0.21", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.14", features = ["tonic"] }
tracing-opentelemetry = "0.22"
tokio = { workspace = true }
hyper = { version = "0.14", features = ["server", "tcp", "http1"] }
thiserror.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
```

**Step 2: Create lib.rs**

Create `crates/zcash-stratum-observability/src/lib.rs`:

```rust
//! Observability for Zcash Stratum V2
//!
//! Provides:
//! - Prometheus metrics endpoint
//! - Structured JSON logging
//! - OpenTelemetry distributed tracing

pub mod metrics;
pub mod logging;
pub mod tracing_setup;

pub use metrics::{PoolMetrics, start_metrics_server};
pub use logging::init_logging;
pub use tracing_setup::init_tracing;
```

**Step 3: Create metrics.rs**

Create `crates/zcash-stratum-observability/src/metrics.rs`:

```rust
//! Prometheus metrics for pool monitoring

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec,
    IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
    TextEncoder, Encoder,
};
use std::net::SocketAddr;
use std::sync::Arc;
use hyper::{Body, Request, Response, Server, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use tracing::{info, error};

/// Pool metrics collection
#[derive(Clone)]
pub struct PoolMetrics {
    registry: Registry,

    // Connection metrics
    pub connections_total: IntCounter,
    pub connections_active: IntGauge,
    pub jd_connections_total: IntCounter,
    pub jd_connections_active: IntGauge,

    // Share metrics
    pub shares_submitted: IntCounterVec,
    pub shares_accepted: IntCounter,
    pub shares_rejected: IntCounterVec,

    // Block metrics
    pub blocks_found: IntCounter,
    pub blocks_submitted: IntCounter,

    // Hashrate
    pub estimated_hashrate: Gauge,

    // Latency
    pub share_validation_duration: Histogram,
    pub template_fetch_duration: Histogram,

    // Noise/encryption
    pub noise_handshakes_total: IntCounter,
    pub noise_handshakes_failed: IntCounter,
}

impl PoolMetrics {
    /// Create a new metrics collection
    pub fn new() -> Self {
        let registry = Registry::new();

        let connections_total = IntCounter::new(
            "pool_connections_total",
            "Total miner connections received"
        ).unwrap();

        let connections_active = IntGauge::new(
            "pool_connections_active",
            "Currently active miner connections"
        ).unwrap();

        let jd_connections_total = IntCounter::new(
            "pool_jd_connections_total",
            "Total JD client connections received"
        ).unwrap();

        let jd_connections_active = IntGauge::new(
            "pool_jd_connections_active",
            "Currently active JD client connections"
        ).unwrap();

        let shares_submitted = IntCounterVec::new(
            Opts::new("pool_shares_submitted", "Total shares submitted"),
            &["miner_id"]
        ).unwrap();

        let shares_accepted = IntCounter::new(
            "pool_shares_accepted",
            "Total shares accepted"
        ).unwrap();

        let shares_rejected = IntCounterVec::new(
            Opts::new("pool_shares_rejected", "Total shares rejected"),
            &["reason"]
        ).unwrap();

        let blocks_found = IntCounter::new(
            "pool_blocks_found",
            "Total blocks found"
        ).unwrap();

        let blocks_submitted = IntCounter::new(
            "pool_blocks_submitted",
            "Total blocks submitted to network"
        ).unwrap();

        let estimated_hashrate = Gauge::new(
            "pool_estimated_hashrate",
            "Estimated pool hashrate in H/s"
        ).unwrap();

        let share_validation_duration = Histogram::with_opts(
            HistogramOpts::new(
                "pool_share_validation_duration_seconds",
                "Time to validate a share"
            ).buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1])
        ).unwrap();

        let template_fetch_duration = Histogram::with_opts(
            HistogramOpts::new(
                "pool_template_fetch_duration_seconds",
                "Time to fetch a template from Zebra"
            ).buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5])
        ).unwrap();

        let noise_handshakes_total = IntCounter::new(
            "pool_noise_handshakes_total",
            "Total Noise handshakes attempted"
        ).unwrap();

        let noise_handshakes_failed = IntCounter::new(
            "pool_noise_handshakes_failed",
            "Total Noise handshakes failed"
        ).unwrap();

        // Register all metrics
        registry.register(Box::new(connections_total.clone())).unwrap();
        registry.register(Box::new(connections_active.clone())).unwrap();
        registry.register(Box::new(jd_connections_total.clone())).unwrap();
        registry.register(Box::new(jd_connections_active.clone())).unwrap();
        registry.register(Box::new(shares_submitted.clone())).unwrap();
        registry.register(Box::new(shares_accepted.clone())).unwrap();
        registry.register(Box::new(shares_rejected.clone())).unwrap();
        registry.register(Box::new(blocks_found.clone())).unwrap();
        registry.register(Box::new(blocks_submitted.clone())).unwrap();
        registry.register(Box::new(estimated_hashrate.clone())).unwrap();
        registry.register(Box::new(share_validation_duration.clone())).unwrap();
        registry.register(Box::new(template_fetch_duration.clone())).unwrap();
        registry.register(Box::new(noise_handshakes_total.clone())).unwrap();
        registry.register(Box::new(noise_handshakes_failed.clone())).unwrap();

        Self {
            registry,
            connections_total,
            connections_active,
            jd_connections_total,
            jd_connections_active,
            shares_submitted,
            shares_accepted,
            shares_rejected,
            blocks_found,
            blocks_submitted,
            estimated_hashrate,
            share_validation_duration,
            template_fetch_duration,
            noise_handshakes_total,
            noise_handshakes_failed,
        }
    }

    /// Encode metrics in Prometheus text format
    pub fn encode(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for PoolMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the metrics HTTP server
pub async fn start_metrics_server(addr: SocketAddr, metrics: Arc<PoolMetrics>) {
    let make_svc = make_service_fn(move |_| {
        let metrics = Arc::clone(&metrics);
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                let metrics = Arc::clone(&metrics);
                async move {
                    match req.uri().path() {
                        "/metrics" => {
                            let body = metrics.encode();
                            Ok::<_, hyper::Error>(Response::new(Body::from(body)))
                        }
                        "/health" => {
                            Ok(Response::new(Body::from("OK")))
                        }
                        _ => {
                            let mut response = Response::new(Body::from("Not Found"));
                            *response.status_mut() = StatusCode::NOT_FOUND;
                            Ok(response)
                        }
                    }
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    info!("Metrics server listening on http://{}/metrics", addr);

    if let Err(e) = server.await {
        error!("Metrics server error: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = PoolMetrics::new();

        metrics.connections_total.inc();
        metrics.connections_active.set(5);
        metrics.shares_accepted.inc_by(100);

        let output = metrics.encode();
        assert!(output.contains("pool_connections_total 1"));
        assert!(output.contains("pool_connections_active 5"));
        assert!(output.contains("pool_shares_accepted 100"));
    }

    #[test]
    fn test_share_rejection_labels() {
        let metrics = PoolMetrics::new();

        metrics.shares_rejected.with_label_values(&["invalid_solution"]).inc();
        metrics.shares_rejected.with_label_values(&["stale"]).inc();
        metrics.shares_rejected.with_label_values(&["stale"]).inc();

        let output = metrics.encode();
        assert!(output.contains("invalid_solution"));
        assert!(output.contains("stale"));
    }
}
```

**Step 4: Create logging.rs**

Create `crates/zcash-stratum-observability/src/logging.rs`:

```rust
//! Structured JSON logging configuration

use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Logging format
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    /// Human-readable format (default for development)
    #[default]
    Pretty,
    /// JSON format (for production)
    Json,
}

/// Initialize logging with the specified format
pub fn init_logging(format: LogFormat, default_level: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    match format {
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_span_events(FmtSpan::CLOSE)
                )
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_span_events(FmtSpan::CLOSE)
                )
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    // Logging init can only be done once per process, so we skip actual init in tests

    #[test]
    fn test_log_format_default() {
        use super::LogFormat;
        let format: LogFormat = Default::default();
        assert!(matches!(format, LogFormat::Pretty));
    }
}
```

**Step 5: Create tracing_setup.rs**

Create `crates/zcash-stratum-observability/src/tracing_setup.rs`:

```rust
//! OpenTelemetry distributed tracing setup

use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    runtime,
    trace::{self, RandomIdGenerator, Sampler},
    Resource,
};
use opentelemetry::KeyValue;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use thiserror::Error;

/// Tracing configuration
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Service name for traces
    pub service_name: String,
    /// OTLP endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "zcash-stratum".to_string(),
            otlp_endpoint: None,
            sampling_ratio: 1.0,
        }
    }
}

/// Initialize OpenTelemetry tracing
pub fn init_tracing(config: TracingConfig) -> Result<(), TracingError> {
    let Some(endpoint) = config.otlp_endpoint else {
        // No OTLP endpoint configured, skip tracing setup
        return Ok(());
    };

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(&endpoint)
        )
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::TraceIdRatioBased(config.sampling_ratio))
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(Resource::new(vec![
                    KeyValue::new("service.name", config.service_name),
                ]))
        )
        .install_batch(runtime::Tokio)
        .map_err(TracingError::OpenTelemetry)?;

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

/// Shutdown tracing (flush pending spans)
pub fn shutdown_tracing() {
    global::shutdown_tracer_provider();
}

#[derive(Error, Debug)]
pub enum TracingError {
    #[error("OpenTelemetry error: {0}")]
    OpenTelemetry(#[from] opentelemetry::trace::TraceError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.service_name, "zcash-stratum");
        assert!(config.otlp_endpoint.is_none());
        assert_eq!(config.sampling_ratio, 1.0);
    }
}
```

**Step 6: Verify compilation and tests**

Run: `cargo test -p zcash-stratum-observability`
Expected: All tests pass

**Step 7: Commit**

```bash
git add crates/zcash-stratum-observability/
git commit -m "feat(observability): add metrics, logging, and tracing"
```

---

## Task 5: Integrate Noise into Pool Server

**Files:**
- Modify: `crates/zcash-pool-server/Cargo.toml`
- Modify: `crates/zcash-pool-server/src/config.rs`
- Modify: `crates/zcash-pool-server/src/server.rs`

**Step 1: Update Cargo.toml**

Add to `crates/zcash-pool-server/Cargo.toml` dependencies:

```toml
zcash-stratum-noise = { path = "../zcash-stratum-noise" }
zcash-stratum-observability = { path = "../zcash-stratum-observability" }
```

**Step 2: Update config.rs**

Add to `crates/zcash-pool-server/src/config.rs`:

```rust
use zcash_stratum_noise::PublicKey;
use std::path::PathBuf;

// Add to PoolConfig struct:

    /// Enable Noise encryption for miner connections
    pub noise_enabled: bool,

    /// Path to server private key file (hex-encoded)
    pub noise_private_key_path: Option<PathBuf>,

    /// Enable Noise for JD connections
    pub jd_noise_enabled: bool,

    /// Metrics server address
    pub metrics_addr: Option<SocketAddr>,

    /// Use JSON logging format
    pub json_logging: bool,

    /// OTLP endpoint for distributed tracing
    pub otlp_endpoint: Option<String>,

// Update Default impl:
    noise_enabled: false,
    noise_private_key_path: None,
    jd_noise_enabled: false,
    metrics_addr: Some("127.0.0.1:9090".parse().unwrap()),
    json_logging: false,
    otlp_endpoint: None,
```

**Step 3: Update server.rs - Add imports and fields**

Add to imports in `crates/zcash-pool-server/src/server.rs`:

```rust
use zcash_stratum_noise::{Keypair, NoiseResponder, NoiseStream};
use zcash_stratum_observability::{PoolMetrics, start_metrics_server, init_logging, LogFormat};
use std::sync::Arc;
```

Add fields to PoolServer struct:

```rust
    /// Noise responder for encrypted connections
    noise_responder: Option<NoiseResponder>,
    /// Pool metrics
    metrics: Arc<PoolMetrics>,
```

**Step 4: Update server.rs - Initialize noise and metrics**

In `PoolServer::new()`:

```rust
    // Initialize logging
    let log_format = if config.json_logging { LogFormat::Json } else { LogFormat::Pretty };
    init_logging(log_format, "info");

    // Initialize metrics
    let metrics = Arc::new(PoolMetrics::new());

    // Initialize Noise if enabled
    let noise_responder = if config.noise_enabled {
        let keypair = if let Some(path) = &config.noise_private_key_path {
            // Load from file
            let hex = std::fs::read_to_string(path)
                .map_err(|e| PoolError::Config(format!("Failed to read key: {}", e)))?;
            Keypair::from_private_hex(hex.trim())
                .map_err(|e| PoolError::Config(format!("Invalid key: {}", e)))?
        } else {
            // Generate new keypair
            let kp = Keypair::generate();
            info!("Generated Noise keypair. Public key: {}", kp.public);
            kp
        };
        Some(NoiseResponder::new(keypair))
    } else {
        None
    };
```

**Step 5: Update server.rs - Handle encrypted connections**

In `run()` method, when accepting connections:

```rust
    // Accept new connections
    accept_result = listener.accept() => {
        match accept_result {
            Ok((stream, addr)) => {
                self.metrics.connections_total.inc();

                if let Some(ref responder) = self.noise_responder {
                    // Encrypted connection
                    self.metrics.noise_handshakes_total.inc();
                    let responder = responder.clone();
                    let metrics = Arc::clone(&self.metrics);

                    tokio::spawn(async move {
                        match responder.accept(stream).await {
                            Ok(noise_stream) => {
                                metrics.connections_active.inc();
                                // Handle encrypted session
                                if let Err(e) = handle_encrypted_session(noise_stream).await {
                                    warn!("Encrypted session error: {}", e);
                                }
                                metrics.connections_active.dec();
                            }
                            Err(e) => {
                                metrics.noise_handshakes_failed.inc();
                                warn!("Noise handshake failed from {}: {}", addr, e);
                            }
                        }
                    });
                } else {
                    // Unencrypted connection (existing code)
                    self.metrics.connections_active.inc();
                    // ... existing session handling ...
                }
            }
            // ... error handling ...
        }
    }
```

**Step 6: Start metrics server**

In `run()` method, before the main loop:

```rust
    // Start metrics server
    if let Some(addr) = self.config.metrics_addr {
        let metrics = Arc::clone(&self.metrics);
        tokio::spawn(async move {
            start_metrics_server(addr, metrics).await;
        });
    }
```

**Step 7: Commit**

```bash
git add crates/zcash-pool-server/
git commit -m "feat(pool): integrate Noise encryption and metrics"
```

---

## Task 6: Integrate Noise into JD Server

**Files:**
- Modify: `crates/zcash-jd-server/Cargo.toml`
- Modify: `crates/zcash-jd-server/src/config.rs`
- Modify: `crates/zcash-jd-server/src/server.rs`

**Step 1: Update Cargo.toml**

Add to `crates/zcash-jd-server/Cargo.toml`:

```toml
zcash-stratum-noise = { path = "../zcash-stratum-noise" }
```

**Step 2: Update config.rs**

Add to JdServerConfig:

```rust
    /// Enable Noise encryption for JD client connections
    pub noise_enabled: bool,
```

**Step 3: Update server.rs**

Update `handle_jd_client` to accept either plain or encrypted streams. Create a trait abstraction or use an enum for the stream type.

**Step 4: Commit**

```bash
git add crates/zcash-jd-server/
git commit -m "feat(jd-server): add Noise encryption support"
```

---

## Task 7: Integrate Noise into JD Client

**Files:**
- Modify: `crates/zcash-jd-client/Cargo.toml`
- Modify: `crates/zcash-jd-client/src/config.rs`
- Modify: `crates/zcash-jd-client/src/client.rs`
- Modify: `crates/zcash-jd-client/src/main.rs`

**Step 1: Update Cargo.toml**

Add to `crates/zcash-jd-client/Cargo.toml`:

```toml
zcash-stratum-noise = { path = "../zcash-stratum-noise" }
```

**Step 2: Update config.rs**

Add to JdClientConfig:

```rust
    /// Enable Noise encryption
    pub noise_enabled: bool,
    /// Pool's Noise public key (required if noise_enabled)
    pub pool_public_key: Option<String>,
```

**Step 3: Update client.rs**

In `JdClient::run()`, use NoiseInitiator when noise_enabled:

```rust
    let stream = if self.config.noise_enabled {
        let public_key = PublicKey::from_hex(
            self.config.pool_public_key.as_ref()
                .ok_or(JdClientError::Protocol("Missing pool public key".into()))?
        ).map_err(|e| JdClientError::Protocol(e.to_string()))?;

        let initiator = NoiseInitiator::new(public_key);
        let tcp_stream = TcpStream::connect(self.config.pool_jd_addr).await?;
        StreamType::Encrypted(initiator.connect(tcp_stream).await?)
    } else {
        StreamType::Plain(TcpStream::connect(self.config.pool_jd_addr).await?)
    };
```

**Step 4: Update main.rs**

Add CLI arguments:

```rust
    #[arg(long)]
    noise: bool,

    #[arg(long)]
    pool_public_key: Option<String>,
```

**Step 5: Commit**

```bash
git add crates/zcash-jd-client/
git commit -m "feat(jd-client): add Noise encryption support"
```

---

## Task 8: Add Integration Tests

**Files:**
- Create: `crates/zcash-stratum-noise/tests/integration_tests.rs`
- Create: `tests/noise_integration.rs` (workspace level)

**Step 1: Create noise crate integration tests**

Create `crates/zcash-stratum-noise/tests/integration_tests.rs`:

```rust
//! Integration tests for Noise encryption

use zcash_stratum_noise::{Keypair, NoiseInitiator, NoiseResponder};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn test_multiple_concurrent_connections() {
    let server_keypair = Keypair::generate();
    let server_public = server_keypair.public.clone();
    let responder = NoiseResponder::new(server_keypair);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn server
    let responder_clone = responder.clone();
    tokio::spawn(async move {
        for _ in 0..3 {
            let (stream, _) = listener.accept().await.unwrap();
            let resp = responder_clone.clone();
            tokio::spawn(async move {
                let mut noise = resp.accept(stream).await.unwrap();
                let msg = noise.read_message().await.unwrap();
                noise.write_message(&msg).await.unwrap(); // Echo
            });
        }
    });

    // Connect 3 clients concurrently
    let mut handles = vec![];
    for i in 0..3 {
        let pk = server_public.clone();
        let handle = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let initiator = NoiseInitiator::new(pk);
            let mut noise = initiator.connect(stream).await.unwrap();

            let msg = format!("Hello from client {}", i);
            noise.write_message(msg.as_bytes()).await.unwrap();
            let response = noise.read_message().await.unwrap();
            assert_eq!(response, msg.as_bytes());
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn test_key_persistence() {
    let keypair = Keypair::generate();
    let private_hex = keypair.private_hex();
    let public_hex = keypair.public.to_hex();

    // Restore from hex
    let restored = Keypair::from_private_hex(&private_hex).unwrap();
    assert_eq!(restored.public.to_hex(), public_hex);
}
```

**Step 2: Commit**

```bash
git add crates/zcash-stratum-noise/tests/ tests/
git commit -m "test(noise): add integration tests"
```

---

## Task 9: Documentation and Final Verification

**Files:**
- Create: `crates/zcash-stratum-noise/README.md`
- Create: `crates/zcash-stratum-observability/README.md`
- Update: `README.md` (workspace)

**Step 1: Create Noise README**

Create `crates/zcash-stratum-noise/README.md`:

```markdown
# zcash-stratum-noise

Noise Protocol encryption for Zcash Stratum V2.

## Overview

Implements the Noise NK handshake pattern as used by SV2:
- Server has static keypair (public key shared with clients)
- Client uses ephemeral keys per connection
- ChaCha20-Poly1305 encryption with BLAKE2s

## Usage

### Server Side

```rust
use zcash_stratum_noise::{Keypair, NoiseResponder};

let keypair = Keypair::generate();
println!("Public key: {}", keypair.public);

let responder = NoiseResponder::new(keypair);
let encrypted_stream = responder.accept(tcp_stream).await?;
```

### Client Side

```rust
use zcash_stratum_noise::{NoiseInitiator, PublicKey};

let server_key = PublicKey::from_hex("...")?;
let initiator = NoiseInitiator::new(server_key);
let encrypted_stream = initiator.connect(tcp_stream).await?;
```

## Key Management

- Generate: `Keypair::generate()`
- Export: `keypair.private_hex()` / `keypair.public.to_hex()`
- Import: `Keypair::from_private_hex()` / `PublicKey::from_hex()`
```

**Step 2: Create Observability README**

Create `crates/zcash-stratum-observability/README.md`:

```markdown
# zcash-stratum-observability

Observability stack for Zcash Stratum V2.

## Components

### Prometheus Metrics

```rust
use zcash_stratum_observability::{PoolMetrics, start_metrics_server};

let metrics = Arc::new(PoolMetrics::new());
metrics.connections_total.inc();

// Start HTTP server on :9090/metrics
tokio::spawn(start_metrics_server(addr, metrics));
```

### Structured Logging

```rust
use zcash_stratum_observability::{init_logging, LogFormat};

// Development
init_logging(LogFormat::Pretty, "info");

// Production (JSON)
init_logging(LogFormat::Json, "info");
```

### Distributed Tracing

```rust
use zcash_stratum_observability::{init_tracing, TracingConfig};

let config = TracingConfig {
    service_name: "zcash-pool".into(),
    otlp_endpoint: Some("http://localhost:4317".into()),
    sampling_ratio: 0.1,
};
init_tracing(config)?;
```

## Metrics Exposed

| Metric | Type | Description |
|--------|------|-------------|
| `pool_connections_total` | Counter | Total connections |
| `pool_connections_active` | Gauge | Active connections |
| `pool_shares_accepted` | Counter | Accepted shares |
| `pool_shares_rejected` | Counter | Rejected shares |
| `pool_blocks_found` | Counter | Blocks found |
| `pool_estimated_hashrate` | Gauge | Pool hashrate |
```

**Step 3: Update workspace README**

Add Phase 5 status and new crates to the table.

**Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add README.md crates/zcash-stratum-noise/README.md crates/zcash-stratum-observability/README.md
git commit -m "docs: add Phase 5 documentation"
```

---

## Summary

Phase 5 adds two new crates:

1. **`zcash-stratum-noise`** - Noise NK encryption:
   - Keypair generation and storage
   - Handshake (initiator/responder)
   - Encrypted transport stream

2. **`zcash-stratum-observability`** - Production monitoring:
   - Prometheus metrics endpoint
   - Structured JSON logging
   - OpenTelemetry distributed tracing

**Modified crates:**
- `zcash-pool-server` - Optional Noise, metrics integration
- `zcash-jd-server` - Optional Noise for JD connections
- `zcash-jd-client` - Optional Noise client mode

**Configuration:**
- All encryption is configurable (disabled by default for dev)
- Metrics endpoint on :9090/metrics when enabled
- OTLP tracing when endpoint configured

**Not included (Phase 6):**
- Full-Template mode
- Multi-miner JD Client
- Authentication/authorization
