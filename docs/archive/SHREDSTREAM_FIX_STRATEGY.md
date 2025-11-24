# ShredStream Fix Strategy - Nov 10, 2025

## 🔍 Root Cause (Confirmed)

**Issue**: ShredStream disconnects after 30 seconds with 0 entries
**Cause**: Server-side idle timeout + tokio spawn context issue

## 📊 Evidence

### 1. **Grok Analysis**: Server Idle Timeout
- 30-second timeout indicates gRPC server closing idle connections
- Client isn't sending keepalive pings or actively polling fast enough
- Server sees "no activity" and closes connection

### 2. **MEV Bot Comparison**: No Spawn Pattern
- MEV bot: ✅ Works perfectly with ShredStream
- MEV bot: Does NOT use `tokio::spawn` for ShredStream
- MEV bot: Runs stream processing **directly in caller's async context**

**Code Comparison**:

**MEV Bot (WORKING)**:
```rust
pub async fn process(&mut self) -> Result<ShredStreamEvent> {
    let client = ShredstreamClient::connect(&self.endpoint).await?;
    let stream = client.subscribe_entries(request).await?;

    // Direct processing - NO SPAWN
    while let Some(slot_entry_result) = stream.next().await {
        // Process immediately
    }
}
```

**ORE Bot (FAILING)**:
```rust
pub async fn initialize(&mut self) -> Result<()> {
    tokio::spawn(async move {  // <-- SPAWNED TASK
        let client = ShredstreamClient::connect(&endpoint).await?;
        let stream = client.subscribe_entries(request).await?;

        loop {
            match stream.next().await {
                Some(...) => { /* NEVER REACHES HERE */ }
                None => { break; }  // Times out after 30s
            }
        }
    });
}
```

### 3. **Nov 9 Success**: Different Architecture?
- Nov 9: Bot worked with 53,855 entries processed
- Possible changes since Nov 9:
  - Code refactoring that introduced spawn?
  - Different runtime configuration?
  - Network/endpoint changes?

## 🛠️ Fix Options (Ranked by Simplicity)

### Option 1: Remove Spawn + Use Direct Processing ⭐ **RECOMMENDED**

**Approach**: Match MEV bot's pattern - don't spawn ShredStream task

**Benefits**:
- ✅ Simplest fix (remove spawning code)
- ✅ Proven to work (MEV bot uses this)
- ✅ No complex keepalive configuration needed
- ✅ Stream stays in same async context as caller

**Changes Needed**:
1. Remove `tokio::spawn` from `ore_shredstream.rs` initialization
2. Change `process()` to run stream loop directly
3. Remove broadcast channel (not needed without spawn)
4. Process events synchronously in main loop

**Trade-off**: Main loop must call `shredstream.process()` frequently to keep stream active

### Option 2: Add gRPC Keepalive to Spawned Task

**Approach**: Keep spawn pattern but add HTTP/2 keepalive pings

**Benefits**:
- ✅ Maintains current architecture (spawned background task)
- ✅ Prevents server idle timeout

**Challenges**:
- ❌ Requires accessing tonic's `Endpoint` API (not sure if `solana-stream-sdk` exposes this)
- ❌ More complex than Option 1
- ❌ May need to fork/patch `solana-stream-sdk`

**Implementation**:
```rust
// Need to check if SDK supports this
let channel = Endpoint::new(&endpoint)?
    .keep_alive_interval(Duration::from_secs(20))
    .keep_alive_while_idle(true)
    .connect()
    .await?;

let client = ShredstreamClient::new(channel);  // May not be exposed
```

### Option 3: Add Reconnect Logic

**Approach**: Detect 30s timeout and reconnect automatically

**Benefits**:
- ✅ Handles any disconnection (not just idle timeout)
- ✅ Makes bot more resilient

**Challenges**:
- ❌ Doesn't fix root cause (still disconnects every 30s)
- ❌ 30-second gaps in monitoring (miss events)
- ❌ More complex error handling

### Option 4: Aggressive Polling with Timeout

**Approach**: Add timeout to `stream.next()` to force frequent polling

**Implementation**:
```rust
loop {
    match tokio::time::timeout(Duration::from_secs(15), stream.next()).await {
        Ok(Some(entry)) => { /* process */ }
        Ok(None) => { break; }  // Stream ended
        Err(_timeout) => {
            // Still connected, just no data - keeps polling active
            continue;
        }
    }
}
```

**Benefits**:
- ✅ May keep server thinking client is active
- ✅ Minimal code changes

**Challenges**:
- ❌ Doesn't address actual idle timeout (server-side issue)
- ❌ May still timeout if server is strict about keepalive

## 🎯 Recommended Action Plan

**OPTION 1** is the best choice because:
1. **Proven to work** (MEV bot does this successfully)
2. **Simplest implementation** (remove complexity, don't add it)
3. **No SDK limitations** (doesn't require tonic API access)
4. **Matches working patterns** (standalone test also doesn't spawn)

### Implementation Steps:

1. **Refactor `OreShredStreamProcessor`**:
   - Remove `tokio::spawn` from `initialize()`
   - Remove broadcast channel
   - Change `process()` to run synchronous stream loop
   - Return events directly (no channel send/receive)

2. **Update bot main loop**:
   - Poll `shredstream.process()` in every iteration
   - Process returned events immediately

3. **Test**:
   - Run for 60+ seconds to confirm no timeout
   - Verify entries are received continuously

### Alternative: Quick Test of Option 1

Before refactoring, test the concept:

```rust
// In main loop or board sniper
let mut client = ShredstreamClient::connect(endpoint).await?;
let mut stream = client.subscribe_entries(request).await?;

// Try 60-second direct processing
let timeout = tokio::time::timeout(Duration::from_secs(60), async {
    while let Some(result) = stream.next().await {
        // Count entries
    }
});

// If this works for 60s+, Option 1 is confirmed
```

## 📝 Next Steps

1. Test Option 1 (direct processing) in minimal example
2. If successful, refactor `ore_shredstream.rs` to match MEV bot pattern
3. Update bot main loop to call ShredStream directly
4. Remove spawn + broadcast channel complexity
5. Test for 5+ minutes to confirm stability

---

**Status**: Ready to implement Option 1
**Confidence**: HIGH (based on MEV bot success)
**Timestamp**: 2025-11-10 21:45 CST
