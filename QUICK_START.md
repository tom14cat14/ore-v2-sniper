# 🚀 Ore Sniper - Quick Start Guide

Get your Ore grid sniper running in 5 minutes.

## ⚡ Fast Setup

### 1. Configure Environment (2 minutes)

```bash
cd /home/tom14cat14/ORE
cp .env.example .env
nano .env  # or your favorite editor
```

**Minimum required changes in `.env`:**

```bash
# Your wallet private key (REQUIRED)
WALLET_PRIVATE_KEY=YourBase58PrivateKeyHere

# Strategy (defaults are good for testing)
PAPER_TRADING=true              # ✅ Start safe!
ENABLE_REAL_TRADING=false       # ❌ Disabled for safety
```

**Save and exit** (Ctrl+X, Y, Enter in nano)

### 2. Build (1 minute)

```bash
cargo build --release
```

### 3. Run Paper Trading (immediate)

```bash
./target/release/ore_sniper
```

**Expected output:**
```
🎯 Ore Grid Sniper v0.1.0
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ Configuration loaded from environment
⚙️  Configuration Summary:
   Mode: 📝 PAPER TRADING
   Ore API: https://ore.supply/v1/grid
   Min EV: 15.0%
   Snipe window: 3s before reset
   Max claim cost: 0.0500 SOL
   Daily limits: 100 claims, 0.50 SOL max loss
   Jito endpoint: https://ny.mainnet.block-engine.jito.wtf
   Polling interval: 80ms
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🚀 Starting Ore Grid Sniper...
💰 Ore price: 0.00072000 SOL
⏱️  57 seconds until snipe window
```

### 4. Monitor & Test (1+ hour recommended)

Let it run for at least 1 hour to see how it performs.

**What to watch:**
- ✅ "SNIPE TARGET FOUND" messages (finding opportunities)
- ✅ EV percentages (higher = better)
- ✅ Daily statistics in final output
- ❌ Any errors or crashes

**Stop with:** `Ctrl+C`

---

## 📊 Understanding Output

### Normal Operation

```
⏱️  57 seconds until snipe window
```
Waiting for the snipe window (final 3 seconds before reset)

```
🎯 SNIPE TARGET FOUND!
   Square: 12 | Cost: 0.007821 SOL | EV: 18.4% | Expected: 0.009256 SOL
📝 PAPER TRADE: Would claim square 12
   Cost: 0.007821 SOL | Expected: 0.009256 SOL | Net: 0.001435 SOL
```
Found profitable opportunity! In paper mode, just logs it.

### Final Statistics (on exit)

```
📊 Final Statistics:
   Claims: 24
   Successful: 24
   Failed: 0
   Total spent: 0.187704 SOL
   Total earned: 0.222245 SOL
   Net profit: 0.034541 SOL (+18.4%)
```

---

## 🎛️ Tuning Strategy

### If Not Finding Targets

**Problem:** `No profitable targets in window`

**Solution:** Lower EV threshold in `.env`

```bash
MIN_EV_PERCENTAGE=10.0  # Was 15.0
```

### If Finding TOO MANY Targets

**Problem:** Claiming too expensive squares

**Solution:** Tighten limits

```bash
MAX_CLAIM_COST_SOL=0.02  # Was 0.05
MIN_EV_PERCENTAGE=20.0   # Was 15.0
```

### Update Ore Price (Important!)

Default price may be outdated. Get current from Jupiter:

```bash
# Manual update in .env
ORE_PRICE_SOL=0.00085  # Check Jupiter for real price
```

Or wait for Jupiter integration (coming soon).

---

## 🔴 Going Live (After Testing)

**⚠️ ONLY AFTER:**
- ✅ Paper trading runs successfully for 24+ hours
- ✅ You understand the strategy and statistics
- ✅ You have a dedicated wallet with SMALL balance (0.1-0.5 SOL)
- ✅ You've verified Ore program addresses are correct

### Steps

1. **Update `.env`:**
```bash
PAPER_TRADING=false
ENABLE_REAL_TRADING=true
```

2. **Fund wallet** (small amount!)

3. **Run:**
```bash
./target/release/ore_sniper
```

4. **Monitor closely** for first hour

5. **Check wallet balance** regularly:
```bash
solana balance <YOUR_WALLET_ADDRESS>
```

---

## 🛑 Emergency Stop

**If something goes wrong:**

1. `Ctrl+C` to stop immediately
2. Check final statistics
3. Check wallet balance
4. Review logs for errors

**Switch back to paper trading:**
```bash
nano .env
# Set: PAPER_TRADING=true
# Set: ENABLE_REAL_TRADING=false
```

---

## 🐛 Troubleshooting

### "WALLET_PRIVATE_KEY must be set"
You didn't set your wallet key in `.env`

**Fix:**
```bash
nano .env
# Add: WALLET_PRIVATE_KEY=YourKeyHere
```

### "Configuration validation failed"
Both PAPER_TRADING and ENABLE_REAL_TRADING are set wrong.

**Fix:** Ensure exactly one is `true`:
```bash
PAPER_TRADING=true
ENABLE_REAL_TRADING=false
```

### "Failed to fetch grid"
Ore API is down or unreachable.

**Check:** Can you access https://ore.supply/v1/grid in browser?

### "No profitable targets"
EV threshold too high or Ore price wrong.

**Try:**
```bash
MIN_EV_PERCENTAGE=10.0  # Lower threshold
ORE_PRICE_SOL=0.00100   # Update price
```

---

## 📈 Next Steps

After successful paper trading:

1. **Verify Ore program addresses** - Replace placeholders in `ore_program.rs`
2. **Add Jupiter price fetching** - Real-time Ore prices
3. **Test on devnet first** - Before mainnet (requires devnet Ore grid)
4. **Consider GPU solver** - Faster puzzle solving
5. **Add ShredStream** - Ultra-precise timing

See [README.md](README.md) for full documentation.

---

## 📞 Need Help?

1. Check [README.md](README.md) for detailed docs
2. Review code comments in `src/` files
3. Test each component separately
4. Start with small changes

**Remember:** This is real money. Test thoroughly before going live!

---

**Status:** Ready for paper trading ✅
**Version:** v0.1.0
**Last Updated:** 2025-11-09
