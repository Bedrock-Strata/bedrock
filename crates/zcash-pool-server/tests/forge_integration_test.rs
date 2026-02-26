//! Integration tests for forge relay
//!
//! These tests require the "forge" feature to be enabled.

#[cfg(feature = "forge")]
mod forge_tests {
    use zcash_pool_server::config::PoolConfig;
    use zcash_pool_server::ForgeRelay;

    /// Test that ForgeRelay can be created with valid config
    #[test]
    fn test_forge_relay_creation() {
        let mut config = PoolConfig::default();
        config.forge_relay_enabled = true;
        config.forge_relay_peers = vec!["127.0.0.1:8336".parse().unwrap()];
        config.forge_auth_key = Some([0x42; 32]);

        let relay = ForgeRelay::new(&config);
        assert!(relay.is_ok(), "ForgeRelay should create successfully");
    }

    /// Test that ForgeRelay fails with empty peers
    #[test]
    fn test_forge_relay_requires_peers() {
        let mut config = PoolConfig::default();
        config.forge_relay_enabled = true;
        config.forge_relay_peers = vec![]; // Empty!

        let relay = ForgeRelay::new(&config);
        assert!(relay.is_err(), "ForgeRelay should fail with empty peers");
    }
}

/// Test that disabled forge relay doesn't interfere with pool startup
#[test]
fn test_pool_server_without_forge() {
    let config = zcash_pool_server::config::PoolConfig::default();
    // forge_relay_enabled defaults to false

    let server = zcash_pool_server::PoolServer::new(config);
    assert!(server.is_ok(), "Pool server should start without forge relay");
}
