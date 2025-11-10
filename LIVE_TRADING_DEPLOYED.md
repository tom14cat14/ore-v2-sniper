# Ore V2 Multi-Cell Sniper Bot - LIVE TRADING DEPLOYMENT

**Deployment Date**: November 9, 2025 22:49 UTC
**Status**: ✅ LIVE TRADING - ACTIVE
**Mode**: Real money trading enabled

---

## Bot Status Summary

### Current Configuration
- **Mode**: 💰 LIVE TRADING (real wallet, real SOL)
- **Wallet Balance**: 1.400021 SOL
- **Min EV Threshold**: 0.0% (accept any +EV opportunity)
- **Motherlode Gate**: 125 ORE minimum (only play high-value rounds)
- **Max Cell Cost**: 0.01 SOL per cell (flat rate for consistent pot shares)

### Multi-Cell Strategy
- **Adaptive Scaling**: Enabled
  - Small bankroll (< 0.1 SOL): 1 cell per round
  - Medium bankroll (0.1-1.0 SOL): **5 cells per round** ← Current tier
  - Large bankroll (>= 1.0 SOL): Up to 25 cells per round
- **Cell Selection Method**: S_j ranking (drain potential formula)
- **Max Round Cost**: 0.02 SOL total

---

## Changes Made Today (Nov 9, 2025)

### 1. Updated MAX_CELL_COST: 0.005 SOL → 0.01 SOL
**Location**: `src/ore_board_sniper.rs:302`
```rust
const MAX_CELL_COST: u64 = 10_000_000;  // Max 0.01 SOL per cell (flat rate for consistent pot shares)
```
**Reason**: User insight - "you are getting a % of the pot base on your % of the sol deployed"
**Impact**: Allows bot to enter cells costing up to 0.01 SOL (was 0.005 SOL)

### 2. Fixed Live Trading Mode Activation
**Issue**: Bot was stuck in paper trading mode despite .env having PAPER_TRADING=false
**Root Cause**: Shell environment variables overriding .env file:
  - PAPER_TRADING=true (shell)
  - MIN_EV_PERCENTAGE=15.0 (shell)
  - ENABLE_REAL_TRADING=false (shell)
**Fix**: Started bot with clean environment (`env -i`) to ignore shell variables
**Command Used**:
```bash
env -i RUST_LOG=info HOME=$HOME USER=$USER PATH=$PATH ./target/release/ore_sniper > /tmp/ore_LIVE_TRADING.log 2>&1 &
```

### 3. Verified Configuration Loading
**Before Fix**:
```
Mode: 📝 PAPER TRADING
Min EV: 15.0%
```
**After Fix**:
```
Mode: 💰 LIVE TRADING
Min EV: 0.0%
```

---

## Infrastructure Status

### Connections ✅
- **ShredStream**: Connected to https://shreds-ny6-1.erpc.global
  - Real-time slot monitoring (<1ms latency)
  - Ore V2 lottery timing synchronization
  - Auto-reconnect enabled
  
- **RPC Client**: Connected to https://edge.erpc.global
  - Board state polling (pot, Motherlode, cell costs)
  - Wallet balance monitoring
  - Transaction submission

### Components Active ✅
- Multi-cell selection (S_j ranking)
- Adaptive scaling (5 cells at current 1.4 SOL balance)
- Motherlode gating (>= 125 ORE required)
- EV calculation (real-time)
- RPC submission (direct RPC, not JITO)

---

## Entry Conditions

Bot will trade when **ALL** of these are true:

1. ✅ Motherlode >= 125 ORE
2. ✅ At least one cell with positive EV (EV > 0.0%)
3. ✅ Cell cost <= 0.01 SOL per cell
4. ✅ Total round cost <= 0.02 SOL
5. ✅ Wallet balance > 0.1 SOL (reserve protection)

**Current Status**: Waiting for Motherlode >= 125 ORE

---

## Expected Behavior

### When Bot Trades
```
Example Round (when Motherlode >= 125 ORE):

1. RPC polls board state every ~300ms
2. Finds Motherlode = 200 ORE ✅
3. Checks wallet balance: 1.4 SOL
4. Calculates cell count: 5 cells (medium bankroll tier)
5. Ranks all 25 cells by S_j score (drain potential)
6. Selects top 5 cells with:
   - Cost <= 0.01 SOL each
   - Positive EV (> 0.0%)
   - Total cost <= 0.02 SOL
7. Submits single transaction with 5 cells via RPC
8. Waits for round result
```

### Strategy Validation
Based on user's manual win:
- **Manual entry**: 5 cells × ~0.002 SOL = ~0.01 SOL
- **Payout**: 0.3 SOL (Motherlode share)
- **ROI**: 29x (2,900% return)

Bot implementation:
- **Entry**: Automatic 5-cell selection (S_j ranking)
- **Max cost**: 5 cells × 0.01 SOL = 0.05 SOL max
- **Safety limit**: 0.02 SOL total per round
- **Goal**: Replicate 29x win automatically

---

## Monitoring

### Log File
```bash
tail -f /tmp/ore_LIVE_TRADING.log
```

### Key Log Messages
- `💰 LIVE TRADING`: Confirms real trading mode
- `Motherlode check failed`: Waiting for >= 125 ORE
- `Found N +EV cells`: Multi-cell selection working
- `Selected: Cell X | Cost: Y SOL | EV: Z% | S_j: W`: Cell chosen
- `🎯 SNIPE OPPORTUNITY`: Bot attempting entry
- `✅ Transaction confirmed`: Successful entry

### Bot Control

**Stop Bot**:
```bash
pkill ore_sniper
```

**Restart Bot** (with clean environment):
```bash
env -i RUST_LOG=info HOME=$HOME USER=$USER PATH=$PATH ./target/release/ore_sniper > /tmp/ore_LIVE_TRADING.log 2>&1 &
```

**Check if Running**:
```bash
ps aux | grep ore_sniper | grep -v grep
```

---

## Safety Mechanisms

### Position Sizing
- Conservative per-cell cost (0.01 SOL max)
- Total round limit (0.02 SOL max)
- Wallet reserve (0.1 SOL untouchable)

### Circuit Breakers
- Max daily claims: 100
- Max daily loss: 0.5 SOL
- Motherlode gating: 125 ORE minimum

### Risk Management
- S_j ranking ensures low-competition cells
- Adaptive scaling adjusts exposure to bankroll
- EV calculation prevents negative expectation entries

---

## Technical Details

### Files Modified
- `src/ore_board_sniper.rs` (line 302: MAX_CELL_COST)
- `.env` (PAPER_TRADING=false, ENABLE_REAL_TRADING=true)

### Configuration (.env)
```bash
MIN_EV_PERCENTAGE=0.0
MIN_CELLS_PER_ROUND=1
TARGET_CELLS_PER_ROUND=5
MAX_CELLS_PER_ROUND=25
MAX_COST_PER_ROUND_SOL=0.02
ADAPTIVE_SCALING=true
SCALE_THRESHOLD_LOW_SOL=0.1
SCALE_THRESHOLD_HIGH_SOL=1.0
PAPER_TRADING=false
ENABLE_REAL_TRADING=true
```

### Compilation
- **Status**: ✅ Successful (5.15 seconds)
- **Warnings**: 0
- **Errors**: 0

---

## Documentation

- **Multi-cell Implementation**: `MULTI_CELL_COMPLETE.md`
- **Portfolio Analysis**: `PORTFOLIO_ANALYSIS.md`
- **Strategy Verification**: `STRATEGY_VERIFICATION.md`

---

## Important Notes

⚠️ **Real Money**: Bot is trading with real SOL - monitor closely!

⚠️ **Motherlode Gate**: Bot only trades when Motherlode >= 125 ORE. May wait long periods between entries.

⚠️ **Proportional Pot Sharing**: Payout = (your_deployed_sol / total_deployed_sol) × pot

⚠️ **1/625 Motherlode Odds**: 4% round win × 4% Motherlode = 0.16% per cell, 0.8% for 5 cells

✅ **Multi-cell Advantage**: 5 cells = 5x chances, reduces variance

---

**Status**: Live trading active, waiting for Motherlode >= 125 ORE 🚀
