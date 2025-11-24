# ORE Board Sniper

**Spread-based EV+ sniper for ORE V2 lottery protocol**

🟢 **Status**: LIVE TRADING (Real Money)
⚡ **Window**: 300ms (Phase 3 optimization)
🎯 **Strategy**: Exploit spread variance from whale deployments

---

## Quick Start

```bash
# Start bot
./target/release/ore_sniper

# Monitor
tail -f logs/ore_sniper_*.log | grep -E '(🎯|✅ Multi-cell|⚡ E2E)'
```

---

## Documentation

📖 **[Complete Guide](./ORE_SNIPER_GUIDE.md)** - Everything you need to know

**Key Sections:**
- What This Bot Does (spread-based strategy)
- Current Configuration (400ms window, live trading)
- Latency Optimization Journey (500ms → 400ms → 300ms planned)
- EV Calculation (Runner 2.1.1 verified formula)
- Running The Bot (commands, monitoring)
- Troubleshooting (common issues)
- Competition Analysis (observed behavior)

**Supporting Docs:**
- [EV Fix Impact](./EV_FIX_IMPACT.md) - Critical EV calculation fix history
- [Motherlode Impact](./test_motherlode_impact.py) - Motherlode value analysis
- [EV Validation](./test_ev_correct.py) - EV calculation test cases

---

## Current Status (2025-11-24)

**Configuration:**
- Window: 300ms before reset (Phase 3)
- E2E Latency: 110-160ms measured
- Buffer: 140-190ms safety margin
- Position: 0.01 SOL per cell
- Mode: LIVE TRADING ⚠️

**Performance:**
- Wallet: B8RVwTgjgbqXenUJumYKxFgT7zkrYC7gyiHSLJxo7fMn
- Balance: 1.000100 SOL
- Infrastructure: Co-located ERPC + ShredStream
- Fees: Base network only (NO JITO tips)

**Recent Improvements (Phase 3):**
1. Increased WebSocket buffer capacity (16→256) for burst handling
2. Reduced snipe window to 300ms for tighter timing
3. Improved connection stability and reconnection logic

---

## Key Features

✅ **Sub-second precision** - 300ms window with 110-160ms E2E
✅ **Real-time detection** - ShredStream <1ms transaction monitoring
✅ **Verified EV formula** - Runner 2.1.1 spread-based calculation
✅ **Spread exploitation** - Only deploy when whales create variance
✅ **Safety limits** - Circuit breakers, daily loss limits, wallet protection

---

## How It Works

**IMPORTANT**: ORE V2 uses a **proportional pot system** - pot is split among ALL deployers across ALL cells based on their share of total deployment.

1. **Monitor** - ShredStream detects all ORE V2 deployments in real-time
2. **Detect** - Identify **spread variance** when total deployment < pot size
3. **Calculate** - EV = (Your Bet / Total Deployment) × Pot - Your Cost
4. **Execute** - Deploy 0.01 SOL in final 300ms when EV+ detected
5. **Win** - Capture proportional share of pot when total deployment < pot

**The Edge:**
- 300ms window sees final deployment state
- Only execute when pot > total deployment (spread variance)
- Current market: ~break-even (pot ≈ total deployment), waiting for opportunity

---

## File Structure

```
/home/tom14cat14/ORE/
├── README.md                     ← You are here
├── ORE_SNIPER_GUIDE.md          ← Complete documentation
├── EV_FIX_IMPACT.md             ← EV calculation history
├── .env                          ← Configuration (NOT in git)
├── src/                          ← Rust source code
│   ├── ore_board_sniper.rs      ← Main bot logic
│   ├── config.rs                 ← Config management
│   ├── ore_rpc.rs               ← Solana RPC interactions
│   ├── ore_shredstream.rs       ← ShredStream integration
│   └── ...
├── target/release/
│   └── ore_sniper               ← Compiled binary
├── logs/                         ← Execution logs
├── archive/                      ← Old/outdated docs
└── test_*.py                     ← Validation scripts
```

---

## Safety & Security

🔐 **Private Key**: Stored in `.env` (never committed to git)
💰 **Funds**: Small balance (1 SOL), circuit breakers active
⚠️ **Live Trading**: Real money at risk, monitor closely
📊 **Monitoring**: Real-time logs, transaction tracking

---

## Support

**Issues?** Check [troubleshooting section](./ORE_SNIPER_GUIDE.md#-troubleshooting) in the guide.

**Questions?** All details in [ORE_SNIPER_GUIDE.md](./ORE_SNIPER_GUIDE.md).

---

**Last Updated**: 2025-11-24
**Version**: 0.3.1 (300ms window Phase 3, improved stability)
**Status**: 🟢 Operational, awaiting first EV+ opportunity
