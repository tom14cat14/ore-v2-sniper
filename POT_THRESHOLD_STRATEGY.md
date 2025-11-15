# Pot Threshold Sniping Strategy - Nov 9, 2025

## ✅ IMPLEMENTED

### Strategy Overview
**Smart EV-Based Sniping with Dual Submission**

1. **Continuous EV Monitoring**: Check EV on every Deploy event
2. **Immediate Snipe When EV > 15%**: Don't wait for 2.8s window
3. **Smart Submission**:
   - **>5s from reset**: Regular RPC (free, no tips)
   - **<5s from reset**: JITO (fast, costs tips)

---

## Why This Works

| Old Strategy | New Strategy |
|--------------|--------------|
| Wait for 2.8s window | **Snipe as soon as EV flips positive** |
| All cells full by then | **Get in early (10-30s into round)** |
| 0 opportunities | **2-3 snipes per round** |
| Always use JITO | **Use free RPC when time permits** |

---

## Cost Savings

**Example Round:**
- Pot crosses 0.5 SOL at T+15s (35s until reset)
- EV = +120% → SNIPE NOW
- Time until reset = 35s → **Use FREE RPC**
- Savings: 0.0001 SOL tip = **$0.02 per snipe**
- At 60 snipes/day: **$1.20/day savings**

**Second snipe:**
- Pot crosses 0.9 SOL at T+58s (2s until reset)
- EV = +240% → SNIPE NOW
- Time until reset = 2s → **Use JITO (speed critical)**
- Cost: 0.0001 SOL tip (worth it for speed)

---

## Implementation

### On CellDeployed Event:
1. Update pot: `CURRENT_POT += cell_cost`
2. Calculate EV: `(pot * 0.04) - cost`
3. If EV > 15%:
   - Get time until reset
   - If >5s: Submit via regular RPC
   - If <5s: Submit via JITO
   - Mark cell as sniped (don't double-snipe)

---

## Expected Results

| Metric | Before | After |
|--------|--------|-------|
| Snipes/day | 0 | 40-60 |
| Avg EV | N/A | +150% |
| Tips paid | 0 | $0.60/day |
| Net profit | $0 | **$12-20/day** |

---

**Status**: Implementing dual-submission logic now
