# ORE V2 Lottery Bot 🎲

**High-frequency trading bot for the ORE V2 lottery protocol on Solana**

[![Status](https://img.shields.io/badge/status-active-success.svg)]()
[![Mode](https://img.shields.io/badge/mode-paper%20trading-blue.svg)]()
[![Dashboard](https://img.shields.io/badge/dashboard-live-brightgreen.svg)](https://sol-pulse.com/ore)

## 🌐 Live Dashboard

**View real-time bot performance:** [https://sol-pulse.com/ore](https://sol-pulse.com/ore)

- 📊 Real-time EV grid updates (WebSocket, 100ms refresh)
- 💰 Live pot tracking and cell costs
- 📈 Performance metrics and win rate
- ⚡ Sub-millisecond ShredStream latency monitoring

---

## 📖 Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Dashboard & API](#dashboard--api)
- [Recent Fixes](#recent-fixes)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [Safety Features](#safety-features)

---

## 🎯 Overview

The ORE V2 Lottery Bot is a high-frequency trading system designed for the ORE protocol's lottery mechanism. It:

- **Calculates Expected Value (EV)** in real-time for all 25 cells
- **Monitors pot accumulation** and deployment activity via ShredStream
- **Executes multi-cell deployments** when positive EV opportunities arise
- **Tracks ORE rewards** (Motherlode) in addition to SOL pot winnings
- **Sub-150ms end-to-end latency** for competitive advantage

---

## ⚡ Quick Start

### 1. Configure Your Wallet

```bash
nano .env
# Replace: WALLET_PRIVATE_KEY=REPLACE_WITH_YOUR_BASE58_PRIVATE_KEY
# With your actual Solana wallet key
```

### 2. Build & Test

```bash
cargo build --release
cargo run --release  # Paper trading mode (SAFE)
```

### 3. Monitor

- **Terminal**: Real-time logs
- **Dashboard**: https://sol-pulse.com/ore

**Full guide**: See `QUICK_START_GUIDE.md`

---

## 🌐 Dashboard & API

### Live Dashboard
**URL**: https://sol-pulse.com/ore

### API Endpoints (`https://api.sol-pulse.com`)

```bash
# HTTP
GET /api/ore/status    # Current bot status
GET /api/ore/events    # Recent events
GET /api/ore/health    # Health check

# WebSocket (real-time)
wss://api.sol-pulse.com/api/ore/ws  # 100ms updates
```

---

## 🔧 Recent Fixes (2025-11-19)

### ✅ 6 Critical Bugs Fixed

1. **Blockhash Stub** → Real RPC (all transactions were failing)
2. **Round ID Calc** → Uses Board account (wrong PDA fixed)
3. **Deploy Amount** → Correctly splits across cells (5x inflation fixed)
4. **Entropy VAR** → Uses Board value (not hardcoded)
5. **Balance Check** → Added pre-transaction validation
6. **Config File** → Created .env with safe defaults

**Details**: `IMPROVEMENTS_SUMMARY.md`, `FIXES_APPLIED.md`, `AUDIT_FINDINGS.md`

---

## ⚙️ Configuration

### Risk Profiles

**Conservative** (Recommended):
```bash
MIN_EV_PERCENTAGE=5.0
DEPLOYMENT_PER_CELL_SOL=0.01
MAX_COST_PER_ROUND_SOL=0.05
```

**Balanced** (Default):
```bash
MIN_EV_PERCENTAGE=0.0
DEPLOYMENT_PER_CELL_SOL=0.01
MAX_COST_PER_ROUND_SOL=0.02
```

**Aggressive**:
```bash
MIN_EV_PERCENTAGE=-2.0
DEPLOYMENT_PER_CELL_SOL=0.02
MAX_COST_PER_ROUND_SOL=0.10
```

---

## 🏗️ Architecture

```
ShredStream (0.25ms) ──┐
WebSocket (Board)   ────┼──> Board State Manager
RPC (Transactions)  ────┘         │
                                  ▼
                           EV Calculator
                                  │
                                  ▼
                           Deploy Engine
                                  │
                                  ▼
                         Dashboard API (WS)
                                  │
                                  ▼
                      https://sol-pulse.com/ore
```

---

## 🛡️ Safety Features

- ✅ Paper trading by default
- ✅ RPC/wallet validation before startup
- ✅ Balance checks before transactions
- ✅ Daily loss limits
- ✅ Clear error messages

---

## 📊 Performance

| Metric | Target | Actual |
|--------|--------|--------|
| ShredStream | <2ms | **0.25ms** ✅ |
| End-to-End | <150ms | **~120ms** ✅ |
| WS Updates | 100ms | **100ms** ✅ |

---

## ⚠️ Disclaimer

**This bot trades real money.**

- Start with paper trading
- Test thoroughly
- Only risk what you can afford to lose
- No guarantees provided

**Use at your own risk.**

---

## 📞 Links

- **Dashboard**: https://sol-pulse.com/ore
- **GitHub**: https://github.com/tom14cat14/ore-v2-sniper
- **Quick Start**: See `QUICK_START_GUIDE.md`

---

*Last Updated: 2025-11-19*
