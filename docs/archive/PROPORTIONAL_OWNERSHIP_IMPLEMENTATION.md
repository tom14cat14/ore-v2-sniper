# Proportional Ownership Implementation - Complete

**Date**: 2025-11-10
**Status**: ✅ IMPLEMENTED & COMPILED

## Summary

Bot now correctly implements Ore V2's proportional ownership model where multiple players can deploy variable amounts to the same cell, with rewards split based on each player's percentage of total deployed.

## Key Discovery

**User Insight**: "I think he bot has wrong info on how cost works for cells. When I played a few rds, it was whatever you wanted. Your amount was just the % of the pool."

This revealed that our bot assumed a fixed cost model, but Ore V2 actually uses proportional pooling.

## Implementation Changes

### 1. Documentation Created

**File**: `/home/tom14cat14/ORE/ORE_V2_MECHANICS.md`

Key points documented:
- Proportional ownership model (variable investments)
- Multiple deployers can pool into same cell
- Reward splitting based on percentage: `your_reward = (pot / 25) * (your_amount / cell_total)`
- Strategy: Wait until 2s remaining to know exact pot and deployment amounts
- Fixed investment amount from config (`MAX_CLAIM_COST_SOL`)

### 2. ShredStream Parsing Updated

**File**: `src/ore_shredstream.rs`

**Changes**:
- Updated `OreEvent::CellDeployed` enum (line 22):
  ```rust
  CellDeployed { cell_id: u8, authority: String, amount_lamports: u64 }
  ```

- Updated Deploy instruction parsing (lines 280-314):
  - Extracts `amount_lamports` from instruction data bytes 1-8 (little-endian u64)
  - Parses `squares` bitmask from bytes 9-12 (little-endian u32)
  - Emits events for each cell in the bitmask with actual deployment amounts
  - Logs: `"🎲 Detected Deploy: cell_id={}, amount={:.6} SOL, authority={}"`

### 3. Cell Tracking Updated

**File**: `src/ore_board_sniper.rs`

**Changes** (lines 943-974):
- Handler now receives `amount_lamports` from ShredStream events
- Tracks `cell.deployed_lamports += amount_lamports` (sum of all deployments to cell)
- Sets `cell.cost_lamports = (config.max_claim_cost_sol * 1e9) as u64` (our fixed investment)
- Updates `cell.difficulty = cell.deployers.len() as u64` (number of deployers)
- Logs: `"→ Cell {} totals: deployed={:.6} SOL, deployers={}"`

### 4. EV Calculation (Already Correct!)

**File**: `src/ore_board_sniper.rs` (lines 569-615)

The EV calculation was ALREADY implementing proportional model correctly:
```rust
let cell_total_after = cell_deployed + p_j;
let my_fraction = if cell_total_after > 0.0 { p_j / cell_total_after } else { 0.0 };
let my_sol_if_win = my_fraction * winnings;
```

This calculates our proportional share of rewards based on our percentage of total deployed.

## How It Works Now

### Real-Time Tracking

1. **ShredStream Detection** (<1ms):
   - Deploy transactions detected instantly
   - Extracts exact amount deployed (e.g., 0.005 SOL, 0.02 SOL, etc.)
   - Updates `cell.deployed_lamports` with running total

2. **Cell State**:
   - `cell.deployed_lamports`: Total amount deployed to this cell by all players
   - `cell.cost_lamports`: Our fixed investment amount (from config)
   - `cell.deployers`: List of all authorities who deployed to this cell
   - `cell.difficulty`: Number of deployers (for pot splitting)

3. **Proportional EV**:
   ```
   our_share = our_amount / (cell_deployed + our_amount)
   expected_value = (pot / 25) * our_share * time_bonus
   profit = expected_value - our_amount
   ev_percentage = (profit / our_amount) * 100
   ```

### Strategy

1. Set fixed investment amount via `MAX_CLAIM_COST_SOL` config
2. ShredStream tracks all deployments in real-time
3. Wait until 2s remaining in round (57-58s elapsed)
4. Calculate proportional EV for each cell based on:
   - Total pot size
   - Amount already deployed to cell
   - Our planned investment
   - Our resulting share percentage
5. Deploy to top N cells by proportional EV

## Configuration

```env
MAX_CLAIM_COST_SOL=0.005  # Our fixed investment per cell (5000 lamports)
```

Bot will invest this amount and receive proportional rewards based on:
```
my_share = 0.005 / (cell_deployed + 0.005)
```

## Files Modified

1. `/home/tom14cat14/ORE/ORE_V2_MECHANICS.md` - Created (documentation)
2. `/home/tom14cat14/ORE/src/ore_shredstream.rs` - Updated enum and parsing
3. `/home/tom14cat14/ORE/src/ore_board_sniper.rs` - Updated cell tracking

## Build Status

✅ **COMPILED SUCCESSFULLY**
- Cargo build completed: 21.30s
- 1 warning (unused import - non-critical)
- 0 errors

## Next Steps

1. ⏳ Start bot and verify ShredStream logs show deployment amounts
2. ⏳ Confirm `cell.deployed_lamports` tracking works correctly
3. ⏳ Verify proportional EV calculations match expected values
4. ⏳ Test execution with correct proportional strategy

## Example Log Output (Expected)

```
✅ Cell 5 deployed: 0.020000 SOL by 9WrFd...
   → Cell 5 totals: deployed=0.020000 SOL, deployers=1

✅ Cell 5 deployed: 0.005000 SOL by CWfwu...
   → Cell 5 totals: deployed=0.025000 SOL, deployers=2

💰 Opportunity found at 57.8s with 2.2s remaining
   Cell 5: EV=+45.2%, our_share=16.7% (0.005/0.030 SOL)
```

## Strategy Validation

User's winning strategy (from manual play):
- Variable investment amounts
- Proportional rewards based on percentage
- "Your amount was just the % of the pool"

This implementation now matches that model exactly.

---

**Implementation Complete**: 2025-11-10
**Verification Pending**: Test with live data
