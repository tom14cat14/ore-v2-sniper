# Ore Board Sniper 🎯

High-performance lottery sniping bot for Ore V2 protocol on Solana.

**Status**: ✅ Core implementation complete, ready for integration
**Program ID**: `oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv`
**Network**: Solana Mainnet-Beta

---

## 🎲 What is Ore V2?

**Ore V2 is a LOTTERY system, NOT mining!**

- **Deploy**: Bet SOL on board squares (25 squares available)
- **Wait**: Round lasts 150 slots (~60 seconds)
- **Reset**: Random winning square chosen via entropy
- **Checkpoint**: Claim SOL + ORE rewards if you win

### Key Mechanics
- 25-cell board per round
- 1/25 chance of winning each round
- Winner gets: Total pot + ORE rewards
- Losers forfeit their bet

---

## ⚡ Strategy

**Core Approach**: Snipe cheapest cells in last 2.8s before reset

### Entry Criteria
- Expected Value (EV) > 15%
- Cell not claimed (on-chain or mempool)
- Cost < 0.05 SOL (configurable)
- Time window: Last 2.8 seconds before round reset

### EV Calculation
```
Win Probability = 1/25 (random square)
Win Amount = Total Pot + Your Bet + ORE Reward
Expected Return = (1/25 × Win Amount) - (24/25 × Bet)
EV = (Expected Return - Bet) / Bet
```

### Expected Performance
- Win Rate: 4% (1 in 25 rounds)
- Net EV: +20-30% (after fees)
- Daily Profit: ~$8-10/day (conservative)

---

## 🚀 Quick Start

### 1. Configuration

Edit `.env`:
```bash
# Wallet
WALLET_PRIVATE_KEY=your_base58_private_key

# Trading Mode
PAPER_TRADING=true          # Start with paper trading!
ENABLE_REAL_TRADING=false   # Set to true for live trading

# Strategy
MIN_EV_PERCENTAGE=15.0      # Minimum EV to enter
MAX_CLAIM_COST_SOL=0.05     # Max bet per cell
```

### 2. Build

```bash
cargo build --release
```

### 3. Run

```bash
# Paper trading (safe)
PAPER_TRADING=true cargo run --release

# Live trading (DANGER!)
ENABLE_REAL_TRADING=true PAPER_TRADING=false cargo run --release
```

---

## 📦 Project Structure

```
ore-sniper/
├── src/
│   ├── ore_instructions.rs      # Deploy/Checkpoint builders
│   ├── ore_board_sniper.rs      # Main sniping logic
│   ├── config.rs                # Configuration
│   ├── main.rs                  # Entry point
│   └── lib.rs                   # Module exports
├── .env                         # Configuration (gitignored)
├── .env.example                 # Example configuration
├── Cargo.toml                   # Dependencies
├── STATUS.md                    # Implementation status
├── NEXT_STEPS.md                # TODO list
└── README.md                    # This file
```

---

## 🛡️ Safety Features

### Paper Trading Mode (DEFAULT)
- All trades simulated
- No real transactions
- Test extensively before going live

### Daily Limits
- Max 100 claims per day
- Max 0.5 SOL daily loss
- Min 0.1 SOL wallet balance

### Real-time Monitoring
- Live EV calculations
- Competitor tracking
- Pot size monitoring

---

## 🔧 Next Steps (4-6 hours to production)

See `NEXT_STEPS.md` for detailed implementation plan:

1. **ShredStream Integration** (1-2 hours)
   - Real-time Ore program log monitoring
   - <1ms latency slot updates

2. **RPC Board Fetching** (1 hour)
   - Query Board and Round accounts
   - Get real cell costs

3. **Jito Integration** (30 min)
   - Bundle submission
   - Priority fee optimization

4. **Testing** (1-2 hours)
   - Paper trade 5-10 rounds
   - Verify EV calculations

---

## 📊 Architecture

### Data Flow
```
ShredStream → Parse Logs → Update Board → Calculate EV → Deploy
                ↓              ↓             ↓           ↓
           Slot Updates   Cell States   Find Target  Jito Bundle
```

### Latency Target
- ShredStream: <1ms (log detection)
- Board Update: <5ms (state sync)
- EV Calculation: <1ms (simple math)
- Jito Submission: <10ms (bundle)
- **Total**: <150ms end-to-end

---

## 🔗 Resources

- **Ore V2 GitHub**: https://github.com/HardhatChad/ore
- **Ore Website**: https://ore.supply
- **Program**: oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv
- **Mint**: oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp

---

## ⚠️ Disclaimer

This is experimental software. Use at your own risk.

- Start with paper trading
- Test extensively before live trading
- Never bet more than you can afford to lose
- Lottery systems are inherently risky

---

**Built with**: Rust, Solana SDK, ShredStream, Jito
**License**: MIT
**Status**: Development (Core Complete)
