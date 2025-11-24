# ShredStream Disconnection - Grok Analysis (Nov 10, 2025)

## 🔍 Problem Summary

ShredStream connects successfully but disconnects after **exactly 30 seconds** with 0 entries received, despite:
- ✅ Same code works in standalone test (626 entries/0.19s)
- ✅ Connection and subscription succeed
- ✅ `_client_guard` keeps client alive
- ❌ Spawned tokio task receives no data

## 🤖 Grok's Diagnosis

**Root Cause**: **Server-side idle timeout** (30 seconds)

The gRPC stream connection succeeds, but the server closes it after 30 seconds of perceived inactivity because the client isn't sending keepalive pings.

### Why Standalone Works But Spawned Task Fails

1. **Standalone test**: Runs in main async context, immediately polls `stream.next().await` in tight loop
2. **Spawned task**: Bot has multiple async tasks (WebSockets, RPC polling, ShredStream) competing for runtime resources

The standalone test's aggressive polling keeps the connection "active" from the server's perspective (even if not explicitly sending pings). The spawned task context may have enough latency between polls that the server considers it idle.

## 🛠️ Recommended Fixes

### Fix 1: Add gRPC Keepalive (PRIMARY)

Grok's recommendation - add HTTP/2 keepalive pings to prevent idle timeout:

```rust
use solana_stream_sdk::Shredstream Client;
use tonic::transport::{Endpoint, Channel};
use std::time::Duration;

// Create endpoint with keepalive
let channel = Endpoint::new(&endpoint)?
    .connect_timeout(Duration::from_secs(10))
    .keep_alive_interval(Duration::from_secs(20))  // Ping every 20s (< 30s timeout)
    .keep_alive_timeout(Duration::from_secs(5))     // Timeout for ping response
    .keep_alive_while_idle(true)                    // Send pings even without data
    .connect()
    .await?;

let mut client = ShredstreamClient::new(channel);
```

**Problem**: Need to check if `solana-stream-sdk` exposes tonic's Channel API or requires custom connection setup.

### Fix 2: Add Explicit Stream Polling

Ensure stream is being actively polled even when no data arrives:

```rust
loop {
    // Set a timeout to detect idle connections
    match tokio::time::timeout(Duration::from_secs(25), stream.next()).await {
        Ok(Some(slot_entry_result)) => {
            // Process entry
        }
        Ok(None) => {
            warn!("Stream ended (server closed connection)");
            break;
        }
        Err(_timeout) => {
            info!("No data for 25s - connection may be idle");
            // Connection still alive, just no data
            // Keepalive should prevent this
        }
    }
}
```

### Fix 3: Reconnect Logic

Add automatic reconnection on timeout:

```rust
loop {
    // Connect/subscribe
    let result = run_stream(&endpoint).await;

    match result {
        Err(e) if is_timeout_error(&e) => {
            warn!("Connection timed out, reconnecting in 2s...");
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        Err(e) => {
            error!("Fatal error: {}", e);
            break;
        }
        Ok(_) => break,
    }
}
```

## 📊 Additional Diagnostics Grok Suggested

1. **Verify runtime is multi-thread** (already confirmed: `#[tokio::main]` defaults to multi-thread)

2. **Add detailed timing logs**:
```rust
let subscribe_time = Instant::now();
info!("Subscribed at {:?}", subscribe_time);

loop {
    let poll_start = Instant::now();
    match stream.next().await {
        Some(entry) => {
            info!("Received entry after {:?}", poll_start.elapsed());
        }
        None => {
            let total_time = subscribe_time.elapsed();
            warn!("Stream ended after {:?} total", total_time);
        }
    }
}
```

3. **Wireshark capture**: Check if HTTP/2 DATA frames are being sent or just PING/GOAWAY

4. **Check `solana-stream-sdk` version**: Ensure latest version (currently 0.5.1)

## 🎯 Next Steps

1. **Check SDK API**: Investigate if `solana-stream-sdk` exposes tonic Channel configuration
2. **Implement keepalive**: Add HTTP/2 keepalive pings (primary fix)
3. **Add timeout handling**: Detect and log idle connections
4. **Test fix**: Run for 60+ seconds to confirm no disconnection

## 📝 Grok's Key Insights

- 30-second timeout is **very common** in gRPC servers (not specific to ShredStream)
- Server sees "no activity" and closes connection to free resources
- Client must send keepalive pings OR actively poll to stay connected
- Standalone works because tight loop keeps connection "busy"
- Spawned task context has enough latency for server to consider it idle

---

**Timestamp**: 2025-11-10 21:32 CST
**Source**: Grok-4-fast-reasoning analysis
**Session**: 20251110_213237
