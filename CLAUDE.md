# ORE Sniper Bot - Claude Instructions

## CRITICAL: EV Strategy Understanding

**NEVER say "empty cell" or "no empty cell" or "waiting for better conditions" regarding +EV opportunities.**

The ORE lottery EV calculation is about **cell cost differences** - finding cells where your deployment cost is lower than the expected return based on the pot distribution. It has NOTHING to do with:
- Getting a cell "to yourself"
- Finding "empty" cells
- Waiting for cells to be unclaimed

EV is calculated based on:
- Total pot size
- Your deployment amount vs cell's existing deployment
- Probability of winning (1/25)
- Payout structure (99% stake back to winners + 90% of losers' pot split among winners)

## Deploy Instruction Fix (Nov 29, 2025)

The Deploy instruction requires exactly **7 accounts**, NOT 9:
1. Signer
2. Authority
3. Automation PDA
4. Board PDA
5. Miner PDA
6. Round PDA
7. System Program

**DO NOT add entropy_var or entropy_program** - they cause `InvalidAccountData` errors.

## Key Files

- `/home/tom14cat14/ORE/src/ore_instructions.rs` - Deploy instruction builder
- `/home/tom14cat14/ORE/src/ore_board_sniper.rs` - Main sniper logic
- `/home/tom14cat14/ORE/.env` - Configuration
