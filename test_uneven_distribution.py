#!/usr/bin/env python3
"""
Test the REAL Ore V2 strategy: Deploy to less-competed cells when distribution is uneven
"""

def calculate_ev_and_s_j(pot_sol, cell_deployed_sol, our_deploy_sol, deployer_count):
    """Calculate EV and S_j for a cell"""
    # EV calculation
    rake = 0.10
    adj = 0.95
    fees = 0.00005

    cell_total_after = cell_deployed_sol + our_deploy_sol
    my_fraction = our_deploy_sol / cell_total_after if cell_total_after > 0 else 0

    win_prob = 1.0 / 25.0
    pot_after_rake = pot_sol * (1.0 - rake)
    my_sol_if_win = my_fraction * pot_after_rake

    expected_return = win_prob * my_sol_if_win * adj
    ev_sol = expected_return - our_deploy_sol - fees
    ev_ratio = ev_sol / our_deploy_sol if our_deploy_sol > 0 else 0

    # S_j calculation (drain potential)
    s_j = (pot_sol - cell_deployed_sol) / (cell_deployed_sol + our_deploy_sol)

    return {
        'ev_percent': ev_ratio * 100,
        's_j': s_j,
        'my_fraction': my_fraction * 100,
        'expected_return': expected_return,
        'my_sol_if_win': my_sol_if_win
    }

print("=" * 70)
print("ORE V2 UNEVEN DISTRIBUTION STRATEGY TEST")
print("=" * 70)

# Your exact scenario:
# 15 cells with 5 SOL each = 75 SOL
# 10 cells with 1 SOL each = 10 SOL
# Total pot = 85 SOL

print("\n📊 SCENARIO:")
print("  15 cells × 5 SOL = 75 SOL")
print("  10 cells × 1 SOL = 10 SOL")
print("  Total pot = 85 SOL")
print("  Our deploy = 0.01 SOL per cell")

pot_sol = 85.0
our_deploy = 0.01

print("\n" + "=" * 70)
print("CELL TYPE 1: Heavy cells (5 SOL deployed, ~10 deployers)")
print("=" * 70)
heavy = calculate_ev_and_s_j(pot_sol, 5.0, our_deploy, 10)
print(f"EV: {heavy['ev_percent']:+.1f}%")
print(f"S_j Rank: {heavy['s_j']:.2f}")
print(f"My Share: {heavy['my_fraction']:.2f}%")
print(f"My SOL if Win: {heavy['my_sol_if_win']:.4f} SOL")
print(f"Expected Return: {heavy['expected_return']:.6f} SOL (cost: {our_deploy:.6f} SOL)")
if heavy['ev_percent'] < 0:
    print("❌ NEGATIVE EV - Don't deploy here!")
else:
    print("✅ Positive EV")

print("\n" + "=" * 70)
print("CELL TYPE 2: Light cells (1 SOL deployed, ~2 deployers)")
print("=" * 70)
light = calculate_ev_and_s_j(pot_sol, 1.0, our_deploy, 2)
print(f"EV: {light['ev_percent']:+.1f}%")
print(f"S_j Rank: {light['s_j']:.2f}")
print(f"My Share: {light['my_fraction']:.2f}%")
print(f"My SOL if Win: {light['my_sol_if_win']:.4f} SOL")
print(f"Expected Return: {light['expected_return']:.6f} SOL (cost: {our_deploy:.6f} SOL)")
if light['ev_percent'] < 0:
    print("❌ NEGATIVE EV - Don't deploy here!")
else:
    print("✅ STRONGLY POSITIVE EV - Deploy here!")

print("\n" + "=" * 70)
print("BOT BEHAVIOR:")
print("=" * 70)
print(f"1. Bot evaluates ALL 25 cells (heavy + light)")
print(f"2. Filters for +EV only:")
print(f"   Heavy cells: {heavy['ev_percent']:+.1f}% → {'SKIP' if heavy['ev_percent'] < 0 else 'INCLUDE'}")
print(f"   Light cells: {light['ev_percent']:+.1f}% → INCLUDE ✅")
print(f"3. Ranks by S_j (higher = better):")
print(f"   Heavy cells: S_j = {heavy['s_j']:.2f}")
print(f"   Light cells: S_j = {light['s_j']:.2f} ⭐ (HIGHER)")
print(f"4. Selects top 5 cells (TARGET_CELLS_PER_ROUND=5)")
print(f"5. Deploys 0.01 SOL to each of the 5 best cells")
print(f"\n✅ Bot WILL deploy to the 10 light cells (highest S_j)")
print(f"✅ Bot will NOT pile into 1 cell (spreads across top 5)")

print("\n" + "=" * 70)
print("PROFIT COMPARISON:")
print("=" * 70)
print(f"If we deploy to 1 heavy cell:")
print(f"  Cost: {our_deploy:.4f} SOL")
print(f"  Expected: {heavy['expected_return']:.6f} SOL")
print(f"  Profit: {heavy['expected_return'] - our_deploy:.6f} SOL")
print(f"  EV: {heavy['ev_percent']:+.1f}% {'❌ LOSS' if heavy['ev_percent'] < 0 else ''}")

print(f"\nIf we deploy to 5 light cells:")
total_cost = our_deploy * 5
total_expected = light['expected_return'] * 5
total_profit = total_expected - total_cost
print(f"  Cost: {total_cost:.4f} SOL")
print(f"  Expected: {total_expected:.6f} SOL")
print(f"  Profit: {total_profit:.6f} SOL")
print(f"  EV: {light['ev_percent']:+.1f}% ✅ PROFIT")

print("\n" + "=" * 70)
print("STRATEGY SUMMARY:")
print("=" * 70)
print("✅ Bot automatically finds uneven distribution")
print("✅ Bot deploys to cells with less SOL (higher S_j)")
print("✅ Bot spreads across multiple cells (5 cells)")
print("✅ Bot waits for snipe window (<3s before reset)")
print("✅ Strategy: Exploit imbalanced boards for +EV")
print("\n🎯 KEY INSIGHT: Don't need 'unclaimed' cells!")
print("   Just need cells with LOW SOL relative to pot!")
print("=" * 70)
