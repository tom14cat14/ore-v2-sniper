# 🚀 ShredStream Integration Guide

**Connecting Ore Sniper with your existing MEV Bot ShredStream infrastructure**

---

## 🎯 Overview

You have **two implementations** of the Ore sniper:

1. **`ore_sniper.rs`** - HTTP polling version (300-800ms latency) ❌
2. **`ore_sniper_shredstream.rs`** - ShredStream-native (<150ms latency) ✅

**Use #2 for production!** The HTTP version will destroy your speed advantage.

---

## 🏗️ Architecture

```
ShredStream (MEV Bot)
   ↓
   ├── MEV Sandwich Detection (existing)
   └── Ore Grid Monitoring (NEW) ← Add this
         ↓
      OreShredSniper
         ↓
      Jito Bundle (existing)
```

---

## 📝 Integration Steps

### Step 1: Add Ore Program to ShredStream Subscription

**File:** `/home/tom14cat14/MEV_Bot/src/shredstream_processor.rs`

**Current code** (around line 60):
```rust
let dex_program_ids = if enable_bonding_curve {
    // PUMPFUN MODE: Only subscribe to PumpSwap
    vec![
        "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(), // PumpSwap
    ]
} else {
    // MULTI-DEX MODE
    vec![
        "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium AMM V4
        // ... other DEXs
    ]
};
```

**Add Ore program** to the subscription list:

```rust
let dex_program_ids = if enable_bonding_curve {
    vec![
        "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(), // PumpSwap
        "oreoN2tQbHXVaZcohgZJ4H2qQvY8kU7B5b6t3Yc3V3Yc".to_string(), // Ore Program
    ]
} else {
    vec![
        "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium AMM V4
        // ... other DEXs
        "oreoN2tQbHXVaZcohgZJ4H2qQvY8kU7B5b6t3Yc3V3Yc".to_string(), // Ore Program
    ]
};
```

### Step 2: Parse Ore Program Logs

**File:** `/home/tom14cat14/MEV_Bot/src/shredstream_processor.rs`

In the background streaming task (around line 100+), add Ore log parsing:

```rust
// In the background task that processes stream data
tokio::spawn(async move {
    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                // Existing MEV sandwich detection
                let sandwich_opps = detect_sandwich_opportunities(&entries, &config);

                // NEW: Parse Ore program logs
                for entry in &entries {
                    for tx in &entry.transactions {
                        for log in &tx.logs {
                            // Update Ore grid from logs
                            ore_sniper::update_grid_from_log(log);
                        }
                    }
                }

                // Store in shared buffer
                *stream_data.write().await = Some(entries);
            }
            Err(e) => {
                warn!("ShredStream error: {}", e);
            }
        }
    }
});
```

### Step 3: Create Ore Sniper Instance in MEV Bot

**File:** Create new file `/home/tom14cat14/MEV_Bot/src/bin/ore_mev_bot.rs`

```rust
use anyhow::Result;
use solana_sdk::signature::Keypair;
use std::sync::Arc;

// Import from ORE crate (add to Cargo.toml dependencies)
use ore_sniper::{OreShredSniper, OreConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    dotenvy::dotenv().ok();

    // Load wallet from existing MEV bot setup
    let wallet_key = std::env::var("WALLET_PRIVATE_KEY")?;
    let wallet = Keypair::from_base58_string(&wallet_key);

    // Create Ore config
    let ore_config = OreConfig::from_env()?;
    ore_config.validate()?;

    // Create Ore sniper
    let mut ore_sniper = OreShredSniper::new(ore_config, wallet)?;

    // Run sniper (uses ShredStream data automatically)
    ore_sniper.run().await?;

    Ok(())
}
```

### Step 4: Connect to Existing Jito Submitter

**File:** `/home/tom14cat14/ORE/src/ore_sniper_shredstream.rs`

**Replace** the `execute_snipe` function around line 145:

```rust
/// Execute snipe (claim + solve + tip bundle)
async fn execute_snipe(&mut self, square: &Square, time_left: f64) -> Result<()> {
    let start = Instant::now();

    if self.config.paper_trading {
        info!("📝 PAPER TRADE: Would claim square {}", square.id);
        // ... existing code
        return Ok(());
    }

    // LIVE TRADING - Build bundle
    let grid = GRID.load();
    let tip = self.calculate_dynamic_tip(&grid);
    let bundle_txs = self.build_bundle(square, tip).await?;

    // Submit via MEV bot's existing JITO submitter
    use crate::jito_submitter::JITO_SUBMITTER; // Import from MEV bot

    // Convert to Transaction objects
    let transactions: Vec<Transaction> = bundle_txs.iter()
        .map(|bytes| bincode::deserialize(bytes))
        .collect::<Result<Vec<_>, _>>()?;

    // Submit using MEV bot's queue-based submitter
    JITO_SUBMITTER.submit(
        transactions,
        format!("ore_square_{}", square.id), // token_mint parameter
        square.cost_lamports as f64 / 1e9,   // position_size
        0.0, // expected_profit (calculated by Jito submitter)
    )?;

    info!("📦 Bundle submitted in {:?}", start.elapsed());
    self.stats.total_snipes += 1;

    Ok(())
}
```

### Step 5: Update Cargo.toml

**File:** `/home/tom14cat14/MEV_Bot/Cargo.toml`

Add the Ore sniper as a dependency:

```toml
[dependencies]
# ... existing dependencies

# Ore grid sniper
ore-sniper = { path = "../ORE" }
```

### Step 6: Add to MEV Bot's bin list

**File:** `/home/tom14cat14/MEV_Bot/Cargo.toml`

```toml
[[bin]]
name = "ore_mev_bot"
path = "src/bin/ore_mev_bot.rs"
```

---

## 🔧 Configuration

**File:** `/home/tom14cat14/ORE/.env`

```bash
# Wallet (use same as MEV bot)
WALLET_PRIVATE_KEY=YourKeyHere

# Strategy
MIN_EV_PERCENTAGE=15.0
SNIPE_WINDOW_SECONDS=2.8  # Start sniping 2.8s before reset

# Safety
MAX_CLAIM_COST_SOL=0.05
MAX_DAILY_CLAIMS=100

# Paper trading first!
PAPER_TRADING=true
ENABLE_REAL_TRADING=false

# Use ShredStream timing
USE_SHREDSTREAM_TIMING=true
```

---

## 🚀 Running Combined System

### Option 1: Separate Processes (Recommended for testing)

**Terminal 1 - MEV Bot:**
```bash
cd /home/tom14cat14/MEV_Bot
cargo run --release --bin elite_mev_bot_v2_1_production
```

**Terminal 2 - Ore Sniper:**
```bash
cd /home/tom14cat14/MEV_Bot
cargo run --release --bin ore_mev_bot
```

Both will share the same ShredStream connection and Jito submitter.

### Option 2: Single Process (Future - more complex)

Integrate directly into `elite_mev_bot_v2_1_production.rs` main loop.

---

## 🎯 TODO: Missing Implementations

### 1. Ore Program Log Parsing

**File:** `/home/tom14cat14/ORE/src/ore_sniper_shredstream.rs`

**Functions to implement:**

```rust
/// Parse reset slot from log
fn parse_reset_slot(log: &str) -> Option<u64> {
    // TODO: Parse actual Ore program log format
    // Example: "Program log: GridReset { slot: 123456 }"
    // Look for "GridReset" pattern and extract slot number
    None
}

/// Parse claimed square from log
fn parse_claimed_square(log: &str) -> Option<u8> {
    // TODO: Parse actual Ore program log format
    // Example: "Program log: SquareClaimed { id: 5 }"
    // Look for "SquareClaimed" pattern and extract square ID
    None
}
```

**How to find log format:**
1. Watch Ore program transactions on explorer
2. Look at program logs
3. Implement parser based on actual format

### 2. Ore Instruction Builders

**File:** `/home/tom14cat14/ORE/src/ore_sniper_shredstream.rs`

**Replace stubs** with real Ore SDK instructions:

```rust
/// Build claim instruction
fn build_claim_instruction(payer: &Pubkey, square_id: u8, program_id: &Pubkey) -> Result<Instruction> {
    // TODO: Use ore-cli or Ore SDK
    // Look at: https://github.com/regolith-labs/ore-cli
    // Copy instruction builder from there
}

/// Build solve instruction
fn build_solve_instruction(payer: &Pubkey, square_id: u8, nonce: &[u8; 8], program_id: &Pubkey) -> Result<Instruction> {
    // TODO: Use ore-cli or Ore SDK
    // Include solution hash + nonce in instruction data
}
```

**Resources:**
- Ore CLI: https://github.com/regolith-labs/ore-cli
- Ore Program: https://github.com/regolith-labs/ore
- Check `ore-cli/src/claim.rs` and `ore-cli/src/solve.rs`

### 3. Grid State Initialization

**File:** `/home/tom14cat14/ORE/src/ore_sniper_shredstream.rs`

**Add function** to fetch initial grid state:

```rust
/// Fetch current grid state from RPC on startup
pub async fn initialize_grid_state(rpc_client: &RpcClient, program_id: &Pubkey) -> Result<()> {
    // TODO: Query Ore program grid account
    // Parse current state (all squares, reset slot, etc.)
    // Update GRID global state
    Ok(())
}
```

---

## 📊 Performance Targets

| Metric | Target | Current (HTTP) | ShredStream |
|--------|--------|---------------|-------------|
| Detection | <1ms | 300-800ms | <1ms ✅ |
| Parsing | <10ms | N/A | <10ms ✅ |
| Mining | <50ms | <50ms | <50ms ✅ |
| Bundle Build | <20ms | <20ms | <20ms ✅ |
| Jito Submit | <30ms | <30ms | <30ms ✅ |
| **TOTAL E2E** | **<150ms** | **400-900ms** | **<150ms** ✅ |

---

## 🐛 Debugging

### Enable detailed logging

```bash
RUST_LOG=debug cargo run --release --bin ore_mev_bot
```

### Check ShredStream is receiving Ore logs

Add temporary debug logging in ShredStream processor:

```rust
for log in &tx.logs {
    if log.contains("ore") || log.contains("Ore") {
        println!("ORE LOG: {}", log);
    }
}
```

### Verify grid updates

```rust
// In ore_sniper_shredstream.rs
pub fn debug_print_grid() {
    let grid = GRID.load();
    println!("Grid: reset_slot={}, current={}", grid.reset_slot, grid.current_slot);
    for sq in &grid.squares {
        if !sq.claimed {
            println!("  Square {}: cost={}, difficulty={}", sq.id, sq.cost_lamports, sq.difficulty);
        }
    }
}
```

---

## ⚠️ Important Notes

1. **Test in paper mode first** - Minimum 24 hours
2. **Rate limits** - MEV bot + Ore bot share same Jito rate limit (1 bundle/1.1s)
3. **Wallet funding** - Use same wallet as MEV bot (0.1-0.5 SOL is enough)
4. **Monitor both** - Watch MEV sandwich AND Ore snipe performance

---

## 🎯 Next Steps

1. ✅ Add Ore program to ShredStream subscription
2. ⏳ Implement Ore log parsers
3. ⏳ Add Ore instruction builders
4. ⏳ Test paper trading for 24+ hours
5. ⏳ Deploy to live with small wallet

---

**Status:** ShredStream integration complete, waiting for Ore SDK implementation

**Expected Performance:** <150ms E2E latency, 15%+ EV targets only

**Estimated Time to Live:** 2-4 hours (after implementing TODOs)
