//! Encrypted transport stream wrapper
//!
//! Wraps a TcpStream with Noise encryption, providing AsyncRead/AsyncWrite.

use snow::TransportState;
use std::io;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::trace;

/// Maximum message size for Noise transport (64KB - overhead)
const MAX_MESSAGE_SIZE: usize = 65535 - 16; // 16 bytes for AEAD tag

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
        let plaintext_len = {
            // Handle lock poisoning gracefully - continue operating even if another thread panicked
            let mut transport = self.transport.lock().unwrap_or_else(|e| e.into_inner());
            transport
                .read_message(&ciphertext, &mut plaintext)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
        };

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
            // Handle lock poisoning gracefully - continue operating even if another thread panicked
            let mut transport = self.transport.lock().unwrap_or_else(|e| e.into_inner());
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
