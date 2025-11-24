# CRITICAL EV Calculation Fix - Impact Analysis

**Date**: 2025-11-24
**Commit**: 48d28fa

## 🚨 What Was Wrong

### Previous (INCORRECT) Logic:
```
Assumption: Pot is split equally among all 25 cells
Your winnings = (your_bet / cell_total) × (total_pot / 25) × 0.9
```

**Example with 100 SOL pot, Cell has 10 SOL, you bet 1 SOL:**
- Your share: 1/(10+1) = 9.09%
- Cell's pot: 100/25 = 4 SOL
- Your winnings: 9.09% × 4 = 0.36 SOL ❌

### Correct (FIXED) Logic:
```
Reality: Entire pot goes to ONE winning cell
Your winnings = (your_bet / cell_total) × total_pot × 0.9
```

**Same example:**
- Your share: 1/(10+1) = 9.09%
- Your winnings: 9.09% × 100 × 0.9 = **8.18 SOL** ✅

### Magnitude of Error:
**The bot was underestimating winnings by 25×!**

---

## 📊 Impact on Strategy

### Before Fix: Almost Everything Was EV-
With 10 SOL pot, 0.01 SOL bet:

| Cell Total | Your % | OLD Winnings | OLD EV% | EV+? |
|------------|--------|--------------|---------|------|
| 0.00 SOL   | 100%   | 0.360 SOL    | +3500%  | ✅   |
| 0.10 SOL   | 9.09%  | 0.033 SOL    | +227%   | ✅   |
| 0.50 SOL   | 1.96%  | 0.007 SOL    | -29%    | ❌   |
| 1.00 SOL   | 0.99%  | 0.004 SOL    | -64%    | ❌   |

### After Fix: Much More Aggressive
With 10 SOL pot, 0.01 SOL bet:

| Cell Total | Your % | NEW Winnings | NEW EV% | EV+? |
|------------|--------|--------------|---------|------|
| 0.00 SOL   | 100%   | 9.000 SOL    | +35900% | ✅   |
| 0.10 SOL   | 9.09%  | 0.818 SOL    | +227%   | ✅   |
| 0.50 SOL   | 1.96%  | 0.176 SOL    | -29%    | ❌   |
| 1.00 SOL   | 0.99%  | 0.089 SOL    | -64%    | ❌   |

**Key Insight:** Empty/low-bet cells are now MASSIVELY EV+ because you can win the entire pot!

---

## 🎯 EV+ Threshold

**Formula:**
```
EV+ when: your_fraction > your_bet × 27.78 / total_pot

Where:
- your_fraction = your_bet / (cell_total + your_bet)
- 27.78 = 25 / 0.9 (accounting for 1/25 win prob and 10% rake)
```

**Example:** 0.01 SOL bet, 10 SOL pot
- Need: your_fraction > 0.01 × 27.78 / 10 = **2.78%**
- With cell_total = 0.10 SOL: your_fraction = 9.09% ✅ EV+
- With cell_total = 0.50 SOL: your_fraction = 1.96% ❌ EV-

**Practical Guideline:**
```
For cell to be EV+:
cell_total < (total_pot / 27.78) - your_bet

Example: 10 SOL pot, 0.01 SOL bet
cell_total < (10 / 27.78) - 0.01 = 0.35 SOL
```

---

## ⚙️ Bot Behavior Changes

### 1. **More Aggressive on Empty Cells**
- Empty cells (0 deployed) are now EXTREMELY EV+
- Bot will prioritize fresh cells with no competition
- Risk: Everyone thinks this way → first-mover advantage

### 2. **Ignores Crowded Cells**
- Cells with significant deployment are now clearly EV-
- Bot will skip popular cells entirely
- Good: Avoids value traps

### 3. **Multi-Cell Portfolio Strategy More Important**
- With correct EV, spreading across multiple low-bet cells reduces risk
- Winning ANY of your cells = massive payout
- Each cell evaluated independently with accurate math

### 4. **Timing Window Critical**
- Wait until last 2-3 seconds to see final cell distribution
- Deploy to top N cells by EV (now calculated correctly!)
- Avoid cells that became crowded late

---

## 🧪 Testing Recommendations

### Before Going Live:

1. **Paper Trading with New EV**
   - Run for at least 50 rounds
   - Verify cell selection makes sense
   - Check if bot is too aggressive or conservative

2. **Compare to Historical Data**
   - Use old deployment data
   - Calculate what EV WOULD have been with correct formula
   - Validate against actual outcomes

3. **Simulate Different Scenarios**
   ```bash
   python3 test_ev_correct.py
   ```
   - Test various pot sizes
   - Test various cell distributions
   - Verify EV+ threshold calculations

4. **Monitor First 10 Live Rounds**
   - Watch which cells bot chooses
   - Verify proportional share calculations
   - Check actual winnings match expectations

---

## 📈 Expected Performance Impact

### Positive:
- ✅ Accurate EV means better cell selection
- ✅ Will avoid value-trap crowded cells
- ✅ Better risk/reward assessment
- ✅ Multi-cell strategy properly optimized

### Concerns:
- ⚠️ Everyone with correct EV will target same empty cells
- ⚠️ May need to adjust MIN_EV threshold (currently 0%)
- ⚠️ Competition for low-bet cells will be fierce

### Recommended Config Changes:
```env
# Consider raising EV threshold to be more selective
MIN_EV_PERCENTAGE=5.0  # Only play if >5% EV

# Or adjust deployment amount
DEPLOYMENT_PER_CELL_SOL=0.005  # Smaller bets = more cells covered

# Safety limits
MAX_CELLS_PER_ROUND=10  # Cap exposure per round
```

---

## 🔍 Verification Checklist

Before deploying to production:

- [ ] Review test_ev_correct.py output
- [ ] Verify calculations match manual examples
- [ ] Check that empty cells show massive EV+
- [ ] Confirm crowded cells show EV-
- [ ] Run paper trading for 50+ rounds
- [ ] Compare paper results to expected EV
- [ ] Adjust MIN_EV_PERCENTAGE if needed
- [ ] Start with small DEPLOYMENT_PER_CELL_SOL
- [ ] Monitor first 10 live rounds closely

---

## 📝 Notes

- This fix is based on user-confirmed ORE V2 mechanics
- All 25 cells compete, ONE wins, pot goes to winners of that cell
- 10% rake applied to YOUR winnings when you claim
- Test file: `test_ev_correct.py` validates the math
- Code updated: `src/ore_board_sniper.rs::calculate_ev()`

---

**Status**: ✅ Fix committed and pushed
**Next Steps**: Paper trading validation before live deployment
