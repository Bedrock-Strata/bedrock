# zcash-stratum-observability

Observability stack for Zcash Stratum V2.

## Components

### Prometheus Metrics

```rust
use zcash_stratum_observability::{PoolMetrics, start_metrics_server};
use std::sync::Arc;
use std::net::SocketAddr;

let metrics = Arc::new(PoolMetrics::new());
metrics.record_connection();
metrics.record_share_accepted();

// Start HTTP server on :9090/metrics
let addr: SocketAddr = "0.0.0.0:9090".parse().unwrap();
tokio::spawn(start_metrics_server(addr, metrics));
```

### Structured Logging

```rust
use zcash_stratum_observability::{init_logging, LogFormat};

// Development (pretty-printed)
init_logging(LogFormat::Pretty, "info");

// Production (JSON for log aggregation)
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
| `zcash_stratum_pool_connections_total` | Counter | Total miner connections |
| `zcash_stratum_pool_connections_active` | Gauge | Active miner connections |
| `zcash_stratum_pool_jd_connections_total` | Counter | Total JD connections |
| `zcash_stratum_pool_jd_connections_active` | Gauge | Active JD connections |
| `zcash_stratum_pool_shares_submitted_total` | Counter | Shares by difficulty tier |
| `zcash_stratum_pool_shares_accepted_total` | Counter | Accepted shares |
| `zcash_stratum_pool_shares_rejected_total` | Counter | Rejected shares by reason |
| `zcash_stratum_pool_blocks_found_total` | Counter | Blocks found |
| `zcash_stratum_pool_blocks_submitted_total` | Counter | Blocks submitted |
| `zcash_stratum_pool_estimated_hashrate` | Gauge | Pool hashrate (H/s) |
| `zcash_stratum_pool_share_validation_duration_seconds` | Histogram | Share validation latency |
| `zcash_stratum_pool_template_fetch_duration_seconds` | Histogram | Template fetch latency |
| `zcash_stratum_pool_noise_handshakes_total` | Counter | Noise handshakes initiated |
| `zcash_stratum_pool_noise_handshakes_failed_total` | Counter | Failed Noise handshakes |

## HTTP Endpoints

- `/metrics` - Prometheus metrics in text format
- `/health` - Health check endpoint returning JSON status

## License

MIT OR Apache-2.0
