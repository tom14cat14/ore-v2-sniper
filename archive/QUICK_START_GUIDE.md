# ORE V2 Sniper - Quick Start Guide

**Status**: All critical bugs fixed! Ready for testing.
**Last Updated**: 2025-11-19

---

## ⚡ 3-Step Quick Start

### Step 1: Configure Your Wallet

```bash
# Edit .env and replace the wallet key
nano .env

# Replace this line:
WALLET_PRIVATE_KEY=REPLACE_WITH_YOUR_BASE58_PRIVATE_KEY

# With your actual base58 private key
# Get it from Solana CLI:
solana-keygen show ~/.config/solana/id.json
```

### Step 2: Build the Bot

```bash
# Build in release mode (optimized)
cargo build --release
```

### Step 3: Run Paper Trading (Safe Mode)

```bash
# Run in safe mode - no real SOL spent!
cargo run --release
```

**The bot will:**
1. ✅ Check RPC connection
2. ✅ Connect to WebSocket for board updates
3. ✅ Track pot size and cell costs
4. ✅ Calculate EV for each cell
5. ✅ Log paper trades (simulated)

---

## 📊 What You Should See

### Successful Startup:
```
🎯 Ore Board Sniper v0.3.0 - Real Ore V2 Protocol
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ Configuration loaded from environment
⚙️  Configuration Summary:
   Mode: 📝 PAPER TRADING (SAFE - No real SOL spent)
   Min EV: 0.0%
   Snipe window: 2s before reset
   Deployment per cell: 0.0100 SOL
   ...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🔍 Performing startup health checks...
   Checking RPC connection...
   ✅ RPC connection healthy
   ✅ RPC responsive (current slot: 12345678)
✅ All health checks passed
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🔧 Initializing Ore Board Sniper...
📊 Dashboard writer initialized
💰 ORE price fetcher initialized (Jupiter API)
📡 Board WebSocket subscriber spawned
📡 Round WebSocket subscriber spawned (round 123)
📡 Treasury WebSocket subscriber spawned
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🚀 Starting Ore Board Sniper...
📊 Strategy: Snipe cheapest cells in final window
⚡ Target latency: <150ms E2E
```

### During Operation:
```
⏱️  15.3s until snipe window | 20 cells free | pot: 0.523000 SOL
💎 Motherlode check: 45.23 ORE (need >= 0.0 ORE)
🔍 Cell 3 EV: pot=0.523000, deployed=0.015000, deployers=12, ...
🎯 MULTI-CELL PORTFOLIO: 3 cells selected | Total: 0.030000 SOL
   #1: Cell 3 | Deployed: 0.015000 SOL | Deployers: 12 | EV: 25.3%
   #2: Cell 7 | Deployed: 0.018000 SOL | Deployers: 15 | EV: 22.1%
   #3: Cell 12 | Deployed: 0.020000 SOL | Deployers: 18 | EV: 18.7%
📝 PAPER TRADE: Would deploy to 3 cells (total: 0.030000 SOL)
```

---

## ❌ Common Errors & Solutions

### Error: "WALLET_PRIVATE_KEY must be set"
**Solution**: Edit `.env` and add your wallet private key

### Error: "Invalid WALLET_PRIVATE_KEY"
**Solution**: Make sure you're using base58 format (not JSON array)

### Error: "RPC health check failed"
**Solution**: Check your internet connection, try a different RPC endpoint:
```bash
# Edit .env:
RPC_URL=https://api.mainnet-beta.solana.com
# Or get a free RPC from Helius, QuickNode, etc.
```

### Error: "Cannot enable both ENABLE_REAL_TRADING and PAPER_TRADING"
**Solution**: Edit `.env` and set only one mode:
```bash
# For safe testing (recommended):
PAPER_TRADING=true
ENABLE_REAL_TRADING=false

# For live trading (danger!):
PAPER_TRADING=false
ENABLE_REAL_TRADING=true
```

### Error: "Entropy VAR not initialized"
**Solution**: Wait a few seconds for WebSocket to sync Board state. If it persists, check RPC/WebSocket connectivity.

### Error: "Insufficient wallet balance"
**Solution**: Add more SOL to your wallet, or reduce `DEPLOYMENT_PER_CELL_SOL` in `.env`

---

## 🎯 Understanding the Strategy

### How It Works:
1. **Monitor Board**: Bot watches all 25 cells in real-time
2. **Calculate EV**: For each cell, calculates expected value:
   - Win probability = 1/25 (random selection)
   - Your share = your_deployment / (cell_total + your_deployment)
   - EV = (win_prob × your_share × pot) - your_deployment - fees
3. **Select Cells**: Picks top N cells with highest S_j score (drain potential)
4. **Wait for Window**: Only acts in final 1-2 seconds before reset
5. **Deploy**: Submits transaction to claim selected cells

### Key Metrics:
- **EV**: Expected value (profit/loss per bet)
- **S_j**: Drain potential score (pot / cell_total)
- **Deployers**: Number of people on this cell (affects pot splitting)
- **Deployed**: Total SOL on this cell (affects your share)

---

## 🔒 Safety Features

✅ **Paper Trading Default**: Safe mode enabled by default
✅ **Balance Checks**: Verifies sufficient funds before transactions
✅ **Daily Limits**: Max 100 claims, 0.5 SOL max loss per day
✅ **Min Balance**: Maintains 0.1 SOL minimum wallet balance
✅ **Validation**: Checks Board state before executing
✅ **Health Checks**: Verifies RPC connection at startup

---

## 📈 Going Live (When Ready)

**⚠️ WARNING: Real trading uses REAL SOL!**

### Before Going Live:
1. ✅ Test in paper mode for at least 5-10 rounds
2. ✅ Verify EV calculations look reasonable
3. ✅ Check wallet has enough SOL (minimum 0.2 SOL recommended)
4. ✅ Start with small deployment amounts (0.01 SOL per cell)
5. ✅ Monitor closely for first few rounds

### Enable Live Trading:
```bash
# Edit .env:
PAPER_TRADING=false
ENABLE_REAL_TRADING=true

# Start bot (REAL MONEY!)
cargo run --release
```

### Monitor Performance:
- Watch for successful Deploy transactions
- Track win rate (should be ~4% = 1 in 25)
- Monitor net P&L
- Check transaction fees

---

## 🛠️ Configuration Tuning

### Conservative (Low Risk):
```bash
MIN_CELLS_PER_ROUND=1
MAX_CELLS_PER_ROUND=3
DEPLOYMENT_PER_CELL_SOL=0.005
MAX_COST_PER_ROUND_SOL=0.015
MIN_EV_PERCENTAGE=10.0
```

### Balanced (Medium Risk):
```bash
MIN_CELLS_PER_ROUND=2
MAX_CELLS_PER_ROUND=5
DEPLOYMENT_PER_CELL_SOL=0.01
MAX_COST_PER_ROUND_SOL=0.05
MIN_EV_PERCENTAGE=5.0
```

### Aggressive (High Risk):
```bash
MIN_CELLS_PER_ROUND=3
MAX_CELLS_PER_ROUND=10
DEPLOYMENT_PER_CELL_SOL=0.02
MAX_COST_PER_ROUND_SOL=0.2
MIN_EV_PERCENTAGE=0.0
```

---

## 📊 Monitoring & Logs

### View Logs:
```bash
# Watch logs in real-time
cargo run --release 2>&1 | tee bot.log

# Filter for important events
cargo run --release 2>&1 | grep -E "(SNIPE|PAPER|Cell|EV)"
```

### Dashboard (JSON):
The bot writes status to `/tmp/ore_bot_status.json`:
```bash
# View current status
cat /tmp/ore_bot_status.json | jq .

# Monitor in real-time
watch -n 1 'cat /tmp/ore_bot_status.json | jq .'
```

---

## 🆘 Getting Help

### Check Logs:
- Look for error messages
- Check RPC/WebSocket connection status
- Verify Board state is syncing

### Debug Mode:
```bash
# Enable debug logging
RUST_LOG=debug cargo run --release
```

### Common Issues:
1. **No trades happening**: Check min_ev_percentage, might be too high
2. **"No opportunity"**: Pot size might be too small, or all cells taken
3. **Slow updates**: Check RPC endpoint performance, try different RPC
4. **WebSocket errors**: Network issues, try different WS endpoint

---

## ✅ Checklist Before Live Trading

- [ ] Tested in paper mode for 5-10 rounds
- [ ] EV calculations verified as reasonable
- [ ] Wallet has sufficient SOL (≥0.2 SOL)
- [ ] `DEPLOYMENT_PER_CELL_SOL` set appropriately
- [ ] Daily limits configured (`MAX_DAILY_CLAIMS`, `MAX_DAILY_LOSS_SOL`)
- [ ] Min balance set (`MIN_WALLET_BALANCE_SOL`)
- [ ] RPC endpoint is fast and reliable
- [ ] Understand the risks (lottery = can lose money)
- [ ] Ready to monitor actively for first hour

---

**Remember**: This is a lottery bot. You will lose most rounds (24/25). Profitability comes from winning bigger pots with proportional ownership. Always start with amounts you can afford to lose!

**Good luck! 🎯🚀**
