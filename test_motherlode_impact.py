#!/usr/bin/env python3
"""
Test Motherlode Impact with 10% Rake

Shows the actual SOL value impact of the motherlode with current numbers.
"""

def calculate_motherlode_value(
    your_fraction: float,      # Your % of the winning cell
    motherlode_ore: float,     # Total ORE in motherlode
    ore_price_sol: float,      # ORE price in SOL
    motherlode_trigger: float = 1.0/625.0,  # 1/625 chance
    rake: float = 0.10         # 10% rake on ORE
):
    """Calculate expected motherlode value"""

    # Your share of motherlode (if triggered)
    your_ore = your_fraction * motherlode_ore

    # 10% rake when you claim
    your_ore_after_rake = your_ore * (1.0 - rake)

    # Value in SOL
    value_before_rake = your_ore * ore_price_sol
    value_after_rake = your_ore_after_rake * ore_price_sol

    # Expected value (accounting for trigger probability)
    expected_value_before_rake = value_before_rake * motherlode_trigger
    expected_value_after_rake = value_after_rake * motherlode_trigger

    return {
        "your_ore": your_ore,
        "your_ore_after_rake": your_ore_after_rake,
        "rake_amount_ore": your_ore * rake,
        "value_before_rake_sol": value_before_rake,
        "value_after_rake_sol": value_after_rake,
        "rake_amount_sol": (value_before_rake - value_after_rake),
        "expected_before_rake": expected_value_before_rake,
        "expected_after_rake": expected_value_after_rake,
        "expected_rake_cost": expected_value_before_rake - expected_value_after_rake,
    }

print("=" * 70)
print("MOTHERLODE RAKE IMPACT ANALYSIS")
print("=" * 70)
print()

# Current real numbers
motherlode = 500.0  # 500 ORE in motherlode
ore_price = 0.0008  # ~0.0008 SOL per ORE (example)

print(f"Current Motherlode: {motherlode:.0f} ORE")
print(f"ORE Price: {ore_price:.6f} SOL")
print(f"Motherlode Value: {motherlode * ore_price:.4f} SOL")
print(f"Trigger Probability: 1/625 = {1/625*100:.3f}%")
print()

# Test different scenarios
scenarios = [
    ("Best Case - 100% of cell", 1.0),
    ("Strong Position - 50%", 0.5),
    ("Medium Position - 20%", 0.2),
    ("Small Position - 10%", 0.1),
    ("Tiny Position - 1%", 0.01),
]

print("=" * 70)
print("SCENARIO ANALYSIS")
print("=" * 70)
print()

for name, fraction in scenarios:
    result = calculate_motherlode_value(fraction, motherlode, ore_price)

    print(f"{name}:")
    print(f"  Your share: {fraction*100:.0f}% of cell")
    print(f"  If motherlode triggers:")
    print(f"    You win: {result['your_ore']:.2f} ORE")
    print(f"    10% rake: -{result['rake_amount_ore']:.2f} ORE ({result['rake_amount_sol']:.6f} SOL)")
    print(f"    You keep: {result['your_ore_after_rake']:.2f} ORE ({result['value_after_rake_sol']:.6f} SOL)")
    print(f"  Expected value contribution to EV:")
    print(f"    Before rake: +{result['expected_before_rake']:.8f} SOL")
    print(f"    After rake:  +{result['expected_after_rake']:.8f} SOL")
    print(f"    Rake cost:   -{result['expected_rake_cost']:.8f} SOL")
    print()

print("=" * 70)
print("KEY INSIGHTS")
print("=" * 70)
print()
print(f"1. With {motherlode:.0f} ORE motherlode @ {ore_price:.6f} SOL:")
print(f"   Total value = {motherlode * ore_price:.4f} SOL")
print()
print("2. 10% rake on motherlode:")
print(f"   Even 100% share loses {motherlode * 0.1 * ore_price:.4f} SOL to rake if triggered")
print()
print("3. Expected value impact (1/625 trigger rate):")
result_100 = calculate_motherlode_value(1.0, motherlode, ore_price)
print(f"   100% share adds {result_100['expected_after_rake']:.8f} SOL to EV")
print(f"   10% share adds {calculate_motherlode_value(0.1, motherlode, ore_price)['expected_after_rake']:.8f} SOL to EV")
print()
print("4. Rake impact on EV:")
print(f"   For 100% share: Reduces motherlode EV by {result_100['expected_rake_cost']:.8f} SOL")
print(f"   As % of motherlode EV: {result_100['expected_rake_cost']/result_100['expected_before_rake']*100:.1f}%")
print()
print("5. Bottom line:")
print("   The 10% rake is correctly applied in the code.")
print("   With current motherlode size, it's a small but non-trivial impact.")
print(f"   Max EV reduction from rake: ~{result_100['expected_rake_cost']:.8f} SOL per round")
