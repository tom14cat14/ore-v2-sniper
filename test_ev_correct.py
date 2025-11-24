#!/usr/bin/env python3
"""
Test ORE EV Calculation - Verify Correct Mechanics

CORRECT MECHANICS:
1. 25 cells compete
2. ONE cell wins (1/25 probability)
3. ENTIRE pot goes to winning cell's bettors
4. Within winning cell: your_share = your_bet / cell_total
5. Your winnings = your_share × total_pot
6. 10% rake taken from YOUR winnings when you claim

Example from user:
- Total pot: 10 SOL
- Cell 1 has: 10 SOL in bets (you bet 1 SOL = 10%)
- If Cell 1 wins: You get 10% × 10 SOL = 1 SOL
- After 10% rake: 1 × 0.9 = 0.9 SOL
"""

def calculate_ev_correct(
    your_bet: float,           # Your bet amount (SOL)
    cell_total: float,         # Total bets on this cell (SOL)
    total_pot: float,          # Total pot across all cells (SOL)
    rake: float = 0.10         # 10% rake
) -> dict:
    """Calculate EV using CORRECT mechanics"""

    # Your proportional share of this cell
    cell_total_after = cell_total + your_bet
    your_fraction = your_bet / cell_total_after if cell_total_after > 0 else 0

    # If this cell wins (1/25 chance), you get your_fraction of ENTIRE pot
    win_prob = 1.0 / 25.0
    your_winnings_if_win_before_rake = your_fraction * total_pot
    your_winnings_if_win = your_winnings_if_win_before_rake * (1.0 - rake)

    # Expected value
    expected_return = win_prob * your_winnings_if_win
    ev_sol = expected_return - your_bet
    ev_percent = (ev_sol / your_bet * 100.0) if your_bet > 0 else 0

    return {
        "your_bet": your_bet,
        "cell_total_before": cell_total,
        "cell_total_after": cell_total_after,
        "your_fraction": your_fraction,
        "total_pot": total_pot,
        "win_prob": win_prob,
        "winnings_if_win_before_rake": your_winnings_if_win_before_rake,
        "winnings_if_win": your_winnings_if_win,
        "expected_return": expected_return,
        "ev_sol": ev_sol,
        "ev_percent": ev_percent
    }

# Test Case 1: User's example
print("=" * 60)
print("TEST 1: User's Example")
print("=" * 60)
print("Scenario: 10 SOL pot, Cell has 10 SOL, you bet 1 SOL")
result = calculate_ev_correct(
    your_bet=1.0,
    cell_total=10.0,
    total_pot=10.0
)
print(f"Your fraction of cell: {result['your_fraction']*100:.2f}%")
print(f"If cell wins, you get: {result['winnings_if_win']:.6f} SOL (before rake: {result['winnings_if_win_before_rake']:.6f})")
print(f"Expected return: {result['expected_return']:.6f} SOL")
print(f"EV: {result['ev_sol']:.6f} SOL ({result['ev_percent']:.2f}%)")
print()

# Test Case 2: Empty cell (best case)
print("=" * 60)
print("TEST 2: Empty Cell (Best EV)")
print("=" * 60)
print("Scenario: 100 SOL pot, Cell has 0 SOL, you bet 0.01 SOL")
result = calculate_ev_correct(
    your_bet=0.01,
    cell_total=0.0,
    total_pot=100.0
)
print(f"Your fraction of cell: {result['your_fraction']*100:.2f}%")
print(f"If cell wins, you get: {result['winnings_if_win']:.6f} SOL")
print(f"Expected return: {result['expected_return']:.6f} SOL")
print(f"EV: {result['ev_sol']:.6f} SOL ({result['ev_percent']:.2f}%)")
print()

# Test Case 3: Crowded cell (worst case)
print("=" * 60)
print("TEST 3: Crowded Cell (Worst EV)")
print("=" * 60)
print("Scenario: 100 SOL pot, Cell has 50 SOL, you bet 0.01 SOL")
result = calculate_ev_correct(
    your_bet=0.01,
    cell_total=50.0,
    total_pot=100.0
)
print(f"Your fraction of cell: {result['your_fraction']*100:.4f}%")
print(f"If cell wins, you get: {result['winnings_if_win']:.6f} SOL")
print(f"Expected return: {result['expected_return']:.6f} SOL")
print(f"EV: {result['ev_sol']:.6f} SOL ({result['ev_percent']:.2f}%)")
print()

# Test Case 4: EV+ threshold analysis
print("=" * 60)
print("TEST 4: When is it EV+?")
print("=" * 60)
print("Your bet: 0.01 SOL, Total pot: 10 SOL")
print()
print("Cell Total | Your % | Winnings | EV (SOL) | EV% | EV+?")
print("-" * 60)
for cell_total in [0.0, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0]:
    result = calculate_ev_correct(
        your_bet=0.01,
        cell_total=cell_total,
        total_pot=10.0
    )
    ev_positive = "✅ YES" if result['ev_sol'] > 0 else "❌ NO"
    print(f"{cell_total:8.2f} | {result['your_fraction']*100:5.2f}% | {result['winnings_if_win']:8.6f} | {result['ev_sol']:8.6f} | {result['ev_percent']:6.1f}% | {ev_positive}")

print()
print("=" * 60)
print("KEY INSIGHT:")
print("=" * 60)
print("You need your_fraction × total_pot × 0.9 × (1/25) > your_bet")
print("Simplified: your_fraction × total_pot > your_bet × 25 / 0.9")
print("Or: your_fraction > your_bet × 27.78 / total_pot")
print()
print("Example: 0.01 SOL bet, 10 SOL pot")
print("Need: your_fraction > 0.01 × 27.78 / 10 = 2.78%")
print("This matches Test 4 results above!")
