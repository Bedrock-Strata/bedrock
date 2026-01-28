# Miner Quick Start Guide

Get mining on a Zcash Stratum V2 pool in 5 minutes.

## Prerequisites

- Mining hardware (ASIC, GPU, or CPU)
- Mining software with Stratum V2 support for Zcash Equihash (200,9)
- Pool address and credentials

## Step 1: Get Pool Information

Contact your pool operator for:

| Information | Example | Description |
|-------------|---------|-------------|
| Pool address | `pool.example.com:3333` | Stratum V2 endpoint |
| Pool public key | `a1b2c3d4...` (64 hex chars) | For Noise encryption |
| Your worker name | `wallet.worker1` | Identifies your miner |

## Step 2: Configure Mining Software

### Generic Configuration

```json
{
  "pool": {
    "url": "stratum+tcp://pool.example.com:3333",
    "user": "t1YourZcashAddress.worker1",
    "pass": "x",
    "noise_pubkey": "a1b2c3d4e5f6..."
  },
  "algorithm": "equihash_200_9"
}
```

### Connection URL Formats

```
# Standard (no encryption)
stratum+tcp://pool.example.com:3333

# With Noise encryption (recommended)
stratum+noise://pool.example.com:3333?pubkey=<pool_public_key_hex>

# TLS wrapped (if pool supports)
stratum+ssl://pool.example.com:3334
```

## Step 3: Verify Connection

Successful connection shows:

```
[INFO] Connected to pool.example.com:3333
[INFO] Noise handshake completed
[INFO] Subscribed, extranonce1=a1b2c3d4
[INFO] Authorized as t1YourZcashAddress.worker1
[INFO] Received new job: height=2000000, difficulty=1.0
[INFO] Share accepted (diff=1.0)
```

## Protocol Flow

```
Miner                                Pool
  |                                    |
  |-------- SetupConnection --------->|
  |<------- SetupConnection.Success --|
  |                                    |
  |-------- OpenMiningChannel ------->|
  |<------- OpenMiningChannel.Success-|
  |                                    |
  |<------- NewMiningJob -------------|  (repeated on new blocks)
  |                                    |
  |-------- SubmitSharesStandard ---->|
  |<------- SubmitShares.Success -----|
```

## Understanding Jobs

When you receive a new job, it contains:

```
NewMiningJob {
  job_id: 42,
  prev_hash: [32 bytes],        // Previous block hash
  merkle_root: [32 bytes],      // Transaction merkle root
  block_commitments: [32 bytes], // NU5+ commitments
  version: 5,                   // Block version
  nbits: 0x1d00ffff,           // Difficulty target
  ntime: 1700000000,           // Block timestamp
}
```

Your mining software:
1. Combines these with your assigned NONCE_1
2. Iterates through NONCE_2 values
3. Solves Equihash (200,9) puzzle
4. Submits solutions that meet share difficulty

## Share Submission

```
SubmitSharesStandard {
  channel_id: 1,
  job_id: 42,
  nonce: [32 bytes],           // NONCE_1 + NONCE_2
  ntime: 1700000000,           // May be rolled
  solution: [1344 bytes],      // Equihash solution
}
```

## Difficulty (Vardiff)

The pool adjusts your difficulty automatically:

| Event | Action |
|-------|--------|
| Too many shares/min | Difficulty increases |
| Too few shares/min | Difficulty decreases |
| Block found | Pool broadcasts to all miners |

Target: ~5 shares per minute (configurable by pool)

## Troubleshooting

### "Connection refused"

```bash
# Check pool is reachable
nc -zv pool.example.com 3333

# Check your firewall
sudo iptables -L | grep 3333
```

### "Noise handshake failed"

- Verify pool public key is correct (64 hex characters)
- Ensure your software supports Noise_NK_25519_ChaChaPoly_BLAKE2s
- Check time sync (handshakes can fail with clock skew)

### "Share rejected: stale"

- Normal during block transitions
- If frequent: check network latency to pool
- Consider a geographically closer pool

### "Share rejected: low difficulty"

- Pool difficulty changed, waiting for new job
- Mining software not respecting SetTarget messages

### "Share rejected: invalid solution"

- Hardware error (check for overheating)
- Incorrect algorithm (must be Equihash 200,9)
- Memory errors (run memtest)

## Performance Tuning

### Reduce Latency

```bash
# Check latency to pool
ping pool.example.com

# Use TCP keepalive
# (your mining software should handle this)
```

### Optimize Share Submission

- Enable aggregated share submission if your software supports it
- Use Noise encryption (slightly more overhead but protects against MITM)

## Example Configurations

### EWBF Miner (GPU)

```
--server pool.example.com
--port 3333
--user t1YourZcashAddress.rig1
--pass x
--algo 200_9
```

### Gminer (GPU)

```
--algo equihash200_9
--server stratum+tcp://pool.example.com:3333
--user t1YourZcashAddress.rig1
```

### lolMiner (GPU)

```
--algo EQUI200_9
--pool stratum+tcp://pool.example.com:3333
--user t1YourZcashAddress.rig1
```

## Monitoring Your Miner

### Pool Dashboard

Most pools provide web dashboards showing:
- Hashrate (reported vs effective)
- Shares submitted (valid/stale/invalid)
- Estimated earnings
- Payout history

### Local Monitoring

Your mining software should report:
- Current hashrate
- Accepted/rejected shares
- Temperature and power usage
- Connection status

## Next Steps

- [Mining Software Integration](./mining-software-integration.md) - For software developers
- [JD Client Guide](./jd-client-guide.md) - Run your own template construction
- [Full-Template Mode](./full-template-mode.md) - Select your own transactions
