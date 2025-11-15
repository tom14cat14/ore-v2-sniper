# Ore Derby +EV Strategy - Friend's Analysis (Nov 9, 2025)

## Key Insight
**Not gambling - it's game-theoretic arbitrage!**

SOL lottery becomes +EV grinder via:
1. ORE rewards (pro-rata split)
2. Motherlode accumulation (1/625 odds)
3. Timing edge (ShredStream <200ms)
4. Placement edge (S_j ranking)

---

## Math (The "Ugly Formula")

### Full EV Per Cell:
```
EV = (1/25) × [(T - W_j - rake) / (n_j+1) + P × (1 + M/625) / (25 × (n_j+1))] × adj - p_j - fees
```

**Where:**
- `T` = Total SOL deployed across all 25 blocks
- `W_j` = SOL already deployed to block j
- `n_j` = Number of cells/deployers on block j
- `p_j` = Price to buy next cell on block j
- `M` = Motherlode accumulated ORE (~150-275)
- `P` = ORE price in SOL (~1.67 @ $300 ORE / $180 SOL)
- `rake` = 10% vaulted
- `adj` = 0.95 (variance adjustment)
- `fees` = 0.00005 SOL (Jito + prio)

### S_j Ranking (Brilliant Proxy):
```
S_j = (T - W_j) / [(n_j+1) × p_j]
```

**Interpretation:**
- High S_j = High "drain potential" from losing blocks / low cost for share
- Use S_j to rank +EV blocks → pick highest

---

## Current Bot Issues

### ❌ What We're Doing Wrong:
1. **No Motherlode gating** - Playing when M is too low (ORE term < cost)
2. **Wrong EV calc** - Ignoring ORE rewards entirely
3. **Wrong targeting** - Using lowest `deployed_lamports`, not S_j
4. **No concentration** - Should focus on ONE high-S_j block, not spread
5. **15% threshold** - Should be +0% (any positive EV)

### ✅ What We Have Right:
1. Real-time data (pot, deployed amounts, deployer counts)
2. ShredStream <200ms latency
3. 2s final window timing
4. JITO bundling

---

## What Needs to Change

### 1. Add Motherlode Tracking
- Fetch M from Round account or oracle
- Gate: Only snipe if `M >= 125 ORE` (threshold)
- Higher M = better ORE EV component

### 2. Fetch ORE Price
- Poll Jupiter API for ORE/SOL price
- Update `P` every minute
- Current: ~1.67 SOL/ORE ($300 ORE / $180 SOL)

### 3. New EV Calculation
```rust
fn calculate_full_ev(
    T: u64,          // Total pot
    W_j: u64,        // Block deployed
    n_j: u64,        // Deployers on block
    p_j: u64,        // Cell price
    M: u64,          // Motherlode ORE (in lamports)
    P: f64,          // ORE price in SOL
) -> f64 {
    let sol_component = (T - W_j - 0.1*T) / (n_j + 1);  // Drain losers
    let ore_component = P * (1.0 + M/625.0) / (25.0 * (n_j + 1));
    let expected_return = (sol_component + ore_component) * 0.95;  // 0.95 adj
    let ev = (expected_return / 25.0) - p_j - 0.00005;  // 1/25 win prob
    ev / p_j  // As percentage
}
```

### 4. S_j Ranking
```rust
fn calculate_s_j(
    T: u64,
    W_j: u64,
    n_j: u64,
    p_j: u64,
) -> f64 {
    (T - W_j) as f64 / ((n_j + 1) * p_j) as f64
}
```

Target highest S_j among +EV blocks.

### 5. Lower Threshold
- Change from 15% to **+0%** (any positive EV)
- Add 10% safety margin for mempool changes

---

## Expected Performance

### With Current Data (Nov 9):
- Avg block: n_j=1200, W_j=3.4 SOL, p_j=0.008 SOL → **-2% EV** (rake kills)
- Good gap: n_j=900, W_j=2.5 SOL, p_j=0.005 SOL → **+12% EV**
- Fat M (275): Same gap → **+25% EV**

### Daily (1 Rig):
- 200-400 cells/day (M-gated + gaps)
- 0.0005 SOL EV/cell average
- **+0.1-0.2 SOL/day** (~$18-36)
- With timing edge: **$50+/day**

---

## Implementation Checklist

- [ ] Add Motherlode field to RoundAccount
- [ ] Fetch M from Round account (or oracle)
- [ ] Add ORE price polling (Jupiter API)
- [ ] Implement full EV calculation (SOL + ORE components)
- [ ] Implement S_j ranking
- [ ] Change min_ev threshold to 0%
- [ ] Add M >= 125 gating
- [ ] Update find_snipe_target to use S_j
- [ ] Add mempool abort logic (if delta_n > 5%)
- [ ] Test with M>150 rounds

---

## Grok's Clarifications (CONFIRMED)

### 1. Motherlode Location ✅
- **Account**: Treasury PDA (global)
- **Seeds**: `["treasury"]`
- **Derivation**: `Pubkey::find_program_address(&[b"treasury"], &ORE_PROGRAM).0`
- **Field**: `motherlode_balance` (u64, divide by 1e9 for ORE)
- **Query**: RPC `getAccountInfo(TREASURY_PDA)` → deserialize borsh
- **Update**: Fetch once per round start; changes on MotherlodeHit event (1/625 odds)

### 2. ORE Price ✅
- **Mint**: `oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp` (9 decimals)
- **API**: Jupiter Price API
  - Endpoint: `https://price.jup.ag/v4/price?ids=oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp`
  - Returns: `{"data": {"oreoU2P8...": {"price": 1.67}}}` (P in SOL)
  - Poll: Every 10-30s, cache 5min
- **Fallback**: Birdeye API if rate limited

### 3. ORE Distribution ✅
**CRITICAL:** ORE is **NOT** pro-rata split!

- **1 ORE/round** → ONE random cell winner on winning block (uniform random, not weighted by SOL)
- **Motherlode** → Same winner gets entire M pool if hit (1/625 chance)
- **SOL winnings** → Pro-rata by SOL deployed (your_SOL / W_j)

**Corrected E[ORE per cell]:**
```
E[ORE] = (1 + M/625) / (25 × (n_j+1))
```
Where `n_j+1` = total cells on block j after we deploy

**Implication:** ORE is higher variance (lottery), but still adds ~20-30% upside on fat M rounds

---

**Status:** ✅ All questions answered - ready to implement!
