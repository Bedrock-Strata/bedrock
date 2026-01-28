//! Integration tests for fiber relay

use zcash_pool_server::config::PoolConfig;
use zcash_pool_server::FiberRelay;

/// Test that FiberRelay can be created with valid config
#[test]
fn test_fiber_relay_creation() {
    let mut config = PoolConfig::default();
    config.fiber_relay_enabled = true;
    config.fiber_relay_peers = vec!["127.0.0.1:8336".parse().unwrap()];
    config.fiber_auth_key = Some([0x42; 32]);

    let relay = FiberRelay::new(&config);
    assert!(relay.is_ok(), "FiberRelay should create successfully");
}

/// Test that FiberRelay fails with empty peers
#[test]
fn test_fiber_relay_requires_peers() {
    let mut config = PoolConfig::default();
    config.fiber_relay_enabled = true;
    config.fiber_relay_peers = vec![]; // Empty!

    let relay = FiberRelay::new(&config);
    assert!(relay.is_err(), "FiberRelay should fail with empty peers");
}

/// Test that disabled fiber relay doesn't interfere with pool startup
#[test]
fn test_pool_server_without_fiber() {
    let config = PoolConfig::default();
    // fiber_relay_enabled defaults to false

    let server = zcash_pool_server::PoolServer::new(config);
    assert!(server.is_ok(), "Pool server should start without fiber relay");
}
