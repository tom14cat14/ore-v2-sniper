# Ore V2 Lottery Mechanics

**Date**: 2025-11-10

## CRITICAL: Proportional Ownership Model

### How Cell Deployments Work

**❌ WRONG MODEL** (what bot initially assumed):
- Fixed cost per cell (e.g., 0.001 SOL)
- Binary ownership: you either own a cell or don't
- First to claim wins

**✅ CORRECT MODEL** (actual Ore V2):
- **Variable investment**: Deploy ANY amount to a cell
- **Proportional ownership**: Your % of rewards = your_amount / total_cell_amount
- **Pooling**: Multiple players can deploy to same cell

### Example

Cell 5 has:
- Player A: 0.1 SOL (20% share)
- Player B: 0.4 SOL (80% share)
- Total: 0.5 SOL

If Cell 5 wins and pot is 10 SOL:
- Player A gets: 10 SOL × 20% = 2 SOL
- Player B gets: 10 SOL × 80% = 8 SOL

### Strategy Implications

1. **Fixed Investment Amount**: We choose how much to invest (e.g., 0.001 SOL)

2. **Wait Until 2s Remaining**: At that point we know:
   - Total pot size (from RPC/WebSocket)
   - Amount deployed to each cell (from ShredStream tracking)
   - Our % share if we deploy now
   - True EV for each cell

3. **EV Calculation**:
   ```
   our_share = our_amount / (cell_deployed + our_amount)
   expected_value = (pot / 25) * our_share * time_bonus
   profit = expected_value - our_amount
   ev_percentage = (profit / our_amount) * 100
   ```

4. **Multi-Cell Strategy**:
   - Deploy to top N cells by EV
   - Spread risk across multiple cells
   - Each cell evaluated independently

## ShredStream Integration

- **Detection**: ShredStream detects deployments in <1ms
- **Tracking**: Track `deployed_lamports` for each cell
- **Timing**: Wait until 2s remaining to calculate final EV
- **Execution**: Deploy based on proportional EV calculations

## Implementation Notes

- `Cell.deployed_lamports`: Track total amount deployed to cell (from ShredStream events)
- `Cell.cost_lamports`: Our chosen investment amount (config parameter)
- `Cell.deployers`: Track list of deployers (for pot splitting calculation)
- Final decision at 2s mark based on complete pot/deployment data
