# ORE V2 Sniper - Comprehensive Audit Findings

**Date**: 2025-11-19
**Status**: Multiple critical bugs preventing functionality
**Priority**: HIGH - Bot cannot execute trades in current state

---

## 🚨 CRITICAL ISSUES (Must Fix for Basic Functionality)

### 1. **Missing .env Configuration File** ⚠️ BLOCKING
**File**: `.env` (missing)
**Impact**: Application cannot start
**Severity**: CRITICAL

**Problem**:
- No `.env` file exists in the repository
- Application will fail immediately on startup because `WALLET_PRIVATE_KEY` is required
- User reported "still haven't gotten it to work correctly" - this is likely THE main reason

**Fix**:
```bash
cp .env.example .env
# Then edit .env and add your wallet private key
```

---

### 2. **Round ID Calculation Bug** ⚠️ CAUSES TRANSACTION FAILURES
**File**: `src/ore_board_sniper.rs:885`
**Impact**: All Deploy transactions will fail
**Severity**: CRITICAL

**Problem**:
```rust
// WRONG: Calculates round_id from current_slot
let round_id = (board.current_slot / 150);
```

The round PDA is derived using this calculated round_id, but it doesn't match the actual round_id stored in the Board account. This causes the Deploy instruction to use the wrong Round PDA, making transactions fail with "account not found" or "invalid account" errors.

**Fix**:
```rust
// CORRECT: Use round_id from Board account (synced via RPC/WebSocket)
let round_id = board.round_id;
```

**Location**: Line 885 in `src/ore_board_sniper.rs`

---

### 3. **Blockhash Stub Implementation** ⚠️ CAUSES ALL TRANSACTIONS TO FAIL
**File**: `src/ore_board_sniper.rs:1359-1362`
**Impact**: Every transaction will fail with "Blockhash not found"
**Severity**: CRITICAL

**Problem**:
```rust
async fn fetch_blockhash_from_shredstream() -> Result<solana_sdk::hash::Hash> {
    // TODO: Integrate with ShredStream or RPC
    Ok(solana_sdk::hash::Hash::new_unique())  // ❌ Returns random hash!
}
```

This function returns a random blockhash that doesn't exist on-chain. All transactions will be rejected immediately.

**Fix**:
```rust
async fn fetch_blockhash_from_shredstream() -> Result<solana_sdk::hash::Hash> {
    use solana_client::rpc_client::RpcClient;
    use std::env;

    let rpc_url = env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let rpc = RpcClient::new(rpc_url);

    rpc.get_latest_blockhash()
        .map_err(|e| anyhow::anyhow!("Failed to fetch blockhash: {}", e))
}
```

---

### 4. **ShredStream Deploy Event Amount Parsing** ⚠️ WRONG EV CALCULATIONS
**File**: `src/ore_shredstream.rs:318-329`
**Impact**: Incorrect EV calculations, wrong deployment tracking
**Severity**: HIGH

**Problem**:
```rust
// Parse squares bitmask
let squares = u32::from_le_bytes([...]);

// Get authority and amount
let amount_lamports = u64::from_le_bytes([...]);

// Log all cells in the bitmask
for cell_id in 0..32 {
    if (squares & (1 << cell_id)) != 0 {
        events.push(OreEvent::CellDeployed {
            cell_id: cell_id as u8,
            authority: authority.clone(),
            amount_lamports,  // ❌ WRONG: Uses TOTAL amount for EACH cell!
        });
    }
}
```

If someone deploys 0.1 SOL to 5 cells, this code will record 0.1 SOL for EACH cell (total: 0.5 SOL), when it should record 0.02 SOL per cell (total: 0.1 SOL split across 5 cells).

**Fix**:
```rust
// Count how many cells are being deployed to
let num_cells = squares.count_ones() as u64;
let amount_per_cell = if num_cells > 0 {
    amount_lamports / num_cells
} else {
    0
};

// Log all cells in the bitmask
for cell_id in 0..32 {
    if (squares & (1 << cell_id)) != 0 {
        events.push(OreEvent::CellDeployed {
            cell_id: cell_id as u8,
            authority: authority.clone(),
            amount_lamports: amount_per_cell,  // ✅ Correct per-cell amount
        });
    }
}
```

---

### 5. **Entropy VAR Not Passed to Deploy Instruction** ⚠️ MAY CAUSE FAILURES
**File**: `src/ore_board_sniper.rs:898-904`, `src/ore_instructions.rs:98-103`
**Impact**: Deploy instruction may use wrong entropy VAR address
**Severity**: HIGH

**Problem**:
The Board account contains the correct `entropy_var` address (received via WebSocket), but the `build_deploy_instruction` function re-derives it instead of using the provided value:

```rust
// In ore_instructions.rs:98-103
let (entropy_var_address, _) = Pubkey::find_program_address(
    &[b"var", &board_address.to_bytes(), &0u64.to_le_bytes()],
    &entropy_program_id,
);
```

The Ore protocol might rotate the entropy VAR index (currently 0, but could change). The WebSocket provides the ACTUAL address from the Board account.

**Fix**:
1. Add `entropy_var: Pubkey` parameter to `build_deploy_instruction()`
2. Use `board.entropy_var` instead of deriving it
3. Pass `board.entropy_var` when calling the function

---

## ⚠️ HIGH PRIORITY ISSUES (Impact Stability/Correctness)

### 6. **Confusion Between Cost and Deployment Amount**
**Files**: Multiple files
**Impact**: Logic confusion, potential incorrect behavior
**Severity**: MEDIUM-HIGH

**Problem**:
The codebase mixes two different concepts:
1. `cell.cost_lamports` - Amount already deployed to a cell (what others paid)
2. `deployment_per_cell_sol` - Amount WE will deploy (our bet)

In some places, the code uses `cost_lamports` as if it's the deployment amount, leading to confusion.

**Example Issues**:
- `ore_board_sniper.rs:60` - Comments say `cost_lamports` is DEPRECATED but it's still used
- `ore_rpc.rs:224-228` - Sets `cost_lamports = deployed[i]` which is confusing naming
- `ore_board_sniper.rs:1238-1241` - Sets `cost_lamports` to config value during ShredStream updates

**Fix**: Rename `cost_lamports` to `total_deployed_lamports` to be clear it's the cumulative amount on that cell.

---

### 7. **WebSocket Board Update Returns Dummy Values**
**File**: `src/ore_board_websocket.rs:188-202`
**Impact**: Board state might be stale/incorrect
**Severity**: MEDIUM

**Problem**:
When Board account is 33 bytes (simplified format), the WebSocket parser returns dummy values:
```rust
return Ok(BoardUpdate {
    round_id: 0,        // ❌ Unknown!
    start_slot: 0,      // ❌ Unknown!
    end_slot: 0,        // ❌ Unknown!
    entropy_var: current_round_pda,
});
```

The code later skips these updates (`if board_update.round_id == 0 { continue; }`), but relies on RPC polling every 5 seconds instead. This adds latency.

**Fix**:
- Query the Round PDA to get the actual round_id, start_slot, and end_slot
- Or accept that WebSocket only provides the round PDA and rely on RPC for details

---

### 8. **Cell Difficulty Tracking Issue**
**File**: `src/ore_board_sniper.rs:1244`
**Impact**: Incorrect pot-splitting calculations
**Severity**: MEDIUM

**Problem**:
```rust
cell.difficulty = cell.deployers.len() as u64;
```

This counts the number of deployers tracked locally via ShredStream events. But:
1. If ShredStream misses events (reconnection), count will be wrong
2. The Round account's `count[i]` field is the authoritative source

**Fix**: Always use `count[i]` from Round WebSocket/RPC updates as the source of truth for deployer count.

---

## 📋 CODE QUALITY ISSUES (Best Practices)

### 9. **Missing Async in Sync Context**
**File**: `src/ore_board_sniper.rs:1428`
**Impact**: Test won't compile
**Severity**: LOW

**Problem**:
```rust
#[test]
fn test_ev_calculation() {
    let sniper = OreBoardSniper::new(config).unwrap();  // ❌ new() is async!
    // ...
}
```

**Fix**: Use `#[tokio::test]` instead of `#[test]`

---

### 10. **Hardcoded Values in Test**
**File**: `src/ore_board_sniper.rs:1403-1426`
**Impact**: Test always uses dummy config
**Severity**: LOW

**Fix**: Extract config creation to helper function or use builder pattern.

---

### 11. **Dead Code / Unused Functions**
**Files**: Throughout codebase
**Impact**: Code bloat, confusion
**Severity**: LOW

Many functions are marked `#[allow(dead_code)]`:
- `ore_board_sniper.rs:730-735` - `find_snipe_target()` (legacy single-cell)
- `ore_board_sniper.rs:957-963` - `calculate_dynamic_tip()`
- `ore_board_sniper.rs:1088-1139` - `try_ev_snipe()`
- `ore_board_sniper.rs:1143-1157` - `clone_for_snipe()`

**Fix**: Remove dead code to simplify maintenance.

---

### 12. **Missing Import in ore_shredstream.rs**
**File**: `src/ore_board_websocket.rs:9`
**Impact**: Comment says "For stream.next()" but import is in wrong place
**Severity**: LOW

Minor documentation issue - the comment is misplaced.

---

## 🎯 SIMPLIFICATION OPPORTUNITIES

### 13. **Simplify Cell Cost Logic**
**Current**: Three different values (cost_lamports, deployed_lamports, deployment_per_cell_sol) causing confusion
**Better**:
- `cell.total_deployed`: What's already on the cell (from Round account)
- `config.deployment_per_cell_sol`: What we're betting (from config)
- Remove `cost_lamports` entirely

---

### 14. **Consolidate Board State Updates**
**Current**: Board state updated from 3 sources (ShredStream, WebSocket, RPC) with complex logic
**Better**:
- Primary: WebSocket (real-time, low latency)
- Fallback: RPC (every 5s if WebSocket stale)
- ShredStream: Only for events (Deploy, Reset), not state

---

### 15. **Remove Force Test Mode Complexity**
**Current**: Multiple if/else branches for `force_test_mode` scattered throughout
**Better**:
- Keep test mode simple: just override EV threshold to accept any cell
- Remove special-case logic in main flow

---

## 🔒 SECURITY ISSUES

### 16. **No Wallet Balance Check Before Deploy**
**File**: `src/ore_board_sniper.rs:869-954`
**Impact**: Could try to deploy more SOL than available
**Severity**: MEDIUM

**Current**: Only checks balance for cell count calculation, but doesn't verify sufficient balance before transaction

**Fix**: Add explicit check:
```rust
if wallet_balance < total_cost + 0.01 {  // 0.01 for fees
    return Err(anyhow!("Insufficient balance: need {:.6}, have {:.6}",
                       total_cost + 0.01, wallet_balance));
}
```

---

## 📊 SUMMARY

### Critical Issues (Must Fix):
1. ✅ Create .env file
2. ✅ Fix round ID calculation (line 885)
3. ✅ Fix blockhash fetching (stub → real RPC)
4. ✅ Fix Deploy event amount parsing (ShredStream)
5. ✅ Fix entropy VAR derivation

### High Priority:
6. Clarify cost vs deployment confusion
7. Handle 33-byte Board format better
8. Fix cell difficulty tracking

### Code Quality:
9. Fix async test
10. Remove dead code
11. Simplify cell cost logic
12. Consolidate state updates

---

## 🚀 RECOMMENDED FIX ORDER

1. **Create .env file** - Enables basic startup
2. **Fix blockhash** - Enables transaction submission
3. **Fix round ID** - Enables correct PDA derivation
4. **Fix Deploy amount parsing** - Enables correct tracking
5. **Fix entropy VAR** - Ensures transaction success
6. **Test with paper trading** - Verify fixes work
7. **Clean up code** - Remove confusion, dead code
8. **Enable real trading** - Go live!

---

**Next Steps**: Begin implementing fixes in priority order.
