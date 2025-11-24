# Portfolio Strategy Analysis - How Many Cells to Buy?

## The Question
**Is buying multiple cells per round +EV, or do you need full board coverage?**

## Observed Results
- **Your win**: 5 cells @ 0.002 SOL = 0.01 SOL → Won 0.3 SOL (30x ROI)
- **Your concern**: If you miss, you lose 0.01 SOL per round until you hit

---

## Critical Clarification: What Resets?

**POT**: Resets every 60s (goes to round winner)
**MOTHERLODE**: ACCUMULATES across rounds (doesn't reset until won!)

So when you buy 5 cells and lose:
- ✅ You lose your 0.01 SOL entry cost
- ✅ But you had 5 chances at the persistent Motherlode
- ✅ Next round, Motherlode is still there (now higher!)

The Motherlode grows every round, making future attempts more valuable.

---

## Portfolio Math: N Cells Per Round

Let's calculate +EV for different coverage levels:

### Assumptions (from current board state):
- Pot = 112 SOL
- Motherlode = 238 ORE (~$124,000 USD or ~750 SOL at $165/SOL)
- Cell cost = 0.002 SOL (average for unclaimed)
- Rake = 10% (vault)
- Your share per cell ≈ 30% (low competition cells)

### Expected Value Formula (Per Round):

```
EV = (N/25) × Pot_Share + (N/625) × Motherlode_Share + N × ORE_Rewards - N × Cost - Fees
```

**Components**:
1. **Pot Component**: `(N/25) × 0.3 × 112 × 0.9 = (N/25) × 30.24 SOL`
2. **Motherlode Component**: `(N/625) × (238 ORE / winners) × 3.14 SOL/ORE`
3. **ORE Rewards**: `N × (1 + 238/625) / 25 × 3.14 SOL ≈ N × 0.173 SOL`
4. **Cost**: `N × 0.002 SOL`

### Simplified Calculation (ignoring ORE rewards for conservatism):

| N Cells | Pot EV | Motherlode EV | Cost | Net EV | ROI |
|---------|--------|---------------|------|--------|-----|
| 1 | 1.21 SOL | 0.018 SOL | 0.002 | +1.23 SOL | +61,400% |
| 5 | 6.05 SOL | 0.09 SOL | 0.010 | +6.13 SOL | +61,300% |
| 10 | 12.10 SOL | 0.18 SOL | 0.020 | +12.26 SOL | +61,300% |
| 25 | 30.24 SOL | 0.45 SOL | 0.050 | +30.64 SOL | +61,280% |

**HOLY SHIT - EVERYTHING IS MASSIVELY +EV!!!**

This can't be right. Let me recalculate with actual competition...

---

## Reality Check: Competition Matters

The above assumes you get 30% of pot share per cell, which only happens on LOW-COMPETITION cells.

### Realistic Scenario:
- Most cells have 10+ deployers
- Your 0.002 SOL vs 0.080 SOL already deployed
- Your share: 0.002 / 0.082 = **2.4%** (not 30%!)

### Recalculation with Competition:

| N Cells | Pot EV (2.4% share) | Motherlode EV | Cost | Net EV | ROI |
|---------|---------------------|---------------|------|--------|-----|
| 1 | 0.097 SOL | 0.018 SOL | 0.002 | +0.113 SOL | +5,650% |
| 5 | 0.485 SOL | 0.09 SOL | 0.010 | +0.565 SOL | +5,650% |
| 10 | 0.970 SOL | 0.18 SOL | 0.020 | +1.130 SOL | +5,650% |
| 25 | 2.425 SOL | 0.45 SOL | 0.050 | +2.825 SOL | +5,650% |

**Still massively +EV!** ROI is constant regardless of coverage.

---

## But Why Did You Win So Much?

Your 0.3 SOL payout suggests you either:
1. Hit a cell with very low competition (high pot share)
2. Got lucky with Motherlode distribution
3. Both

If you got 0.3 SOL from 0.002 SOL entry:
- Pot share alone would be: 0.002 / X = (112 × 0.9) / 25 → X ≈ 0.005 SOL cell total
- Meaning the cell only had 0.003 SOL when you entered!
- That's a **40% pot share** (0.002 / 0.005)

You found an EXTREMELY underdeployed cell - exactly what S_j ranking targets!

---

## The Variance Problem

Even though all N values have same ROI, they differ in:

### Variance (Risk of Ruin):

| Strategy | Cost/Round | Win Every N Rounds | Capital Needed (95% safety) |
|----------|------------|--------------------|-----------------------------|
| 1 cell | 0.002 SOL | ~25 | 0.05 SOL + 3 std dev |
| 5 cells | 0.010 SOL | ~5 | 0.05 SOL + 3 std dev |
| 25 cells | 0.050 SOL | 1 (guaranteed!) | 0.05 SOL (no variance!) |

**Trade-off**:
- More cells = faster wins = lower variance
- Fewer cells = cheaper per round = more attempts with fixed capital

---

## Optimal Strategy

### If You Have Limited Capital (< 0.1 SOL):
**Single cell sniping** (current bot):
- Cost: 0.002 SOL/round
- Can play 50 rounds before reload
- Higher variance but survives longer

### If You Have Medium Capital (0.1 - 0.5 SOL):
**Portfolio approach** (your strategy):
- Cost: 0.01 SOL/round (5 cells)
- Can play 10-50 rounds
- Balanced variance and win rate

### If You Have Large Capital (> 1 SOL):
**Full coverage**:
- Cost: 0.05 SOL/round (25 cells)
- Guaranteed pot win every round
- Zero pot variance, only Motherlode variance

---

## Implementation Recommendation

### Option 1: Keep Single-Cell (Current)
- **Pro**: Lowest capital requirement, highest attempt count
- **Con**: Slower wins, higher variance
- **Best for**: Testing, small bankrolls

### Option 2: Multi-Cell Portfolio
- **Pro**: Faster wins, lower variance, better Motherlode odds
- **Con**: Higher capital requirement
- **Best for**: Once strategy is validated

### Option 3: Adaptive Coverage
- Start with 1 cell (low risk)
- If capital grows, increase to 5 cells
- If capital hits target, max out at 25 cells (guaranteed wins)

---

## Answer to Your Question

> "I think I had 5 cells or so, but if you miss the motherlode it resets and you have to wait so not sure if it really is plus ev, unless after a certain point you are covering the whole board"

**Answer**:

1. ✅ **Motherlode doesn't reset** - it accumulates! Only the pot resets.

2. ✅ **All coverage levels are +EV** (if you target low-competition cells)
   - 1 cell: +5,650% expected ROI
   - 5 cells: +5,650% expected ROI (same %, faster wins)
   - 25 cells: +5,650% expected ROI (guaranteed pot win)

3. ⚠️ **BUT: Only if you pick good cells!**
   - Your 150x win = you found a cell with only 0.003 SOL deployed
   - S_j ranking finds these automatically
   - Random cells with high competition = much lower ROI

4. 🎯 **Optimal strategy depends on capital**:
   - Small bankroll: 1 cell (current bot)
   - Medium bankroll: 5 cells (your approach) ✅ **RECOMMENDED**
   - Large bankroll: 25 cells (guaranteed wins)

---

## Next Steps

Should we modify the bot to support multi-cell deployment?

**Proposed config**:
```env
# Portfolio Strategy
CELLS_PER_ROUND=5           # How many cells to buy per round
CELL_SELECTION=S_J_RANKED   # Buy top 5 S_j cells (not random!)
MAX_COST_PER_ROUND=0.02     # Safety limit (5 × 0.004 = 0.02 SOL max)
```

This would:
- Buy the top 5 S_j cells each round
- Reduce variance (5x better win rate)
- Increase Motherlode odds (5x more chances)
- Cost 5x more per round (but same expected ROI)

**Your call**: Stay with single-cell sniping or upgrade to portfolio strategy?
