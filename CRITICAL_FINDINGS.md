# ⚠️ CRITICAL: Ore Protocol Investigation

## Issue Discovered

After researching the actual Ore protocol, the "grid sniping" strategy described by Grok **does NOT match the real Ore implementation**.

## Actual Ore Protocol (as of 2025)

**Real Instructions:**
- `Mine` - Submit valid proof-of-work hash
- `Claim` - Claim accumulated mining rewards
- `Deploy` - Claim space on board with SOL
- `Checkpoint` - Validate round progress
- `Reset` - Start new mining round

**NOT grid-based sniping** - It's traditional PoW mining with:
1. Find valid hash (CPU/GPU mining)
2. Submit via Mine instruction
3. Accumulate rewards
4. Claim rewards

## What We Built

We built a sniper for a **theoretical grid-based system** that may not exist or may be a different Ore version/fork.

## Next Steps Required

1. **Verify Ore Protocol** - Is there actually a grid/squares system?
2. **Check Ore API** - Does https://ore.supply/v1/grid actually exist?
3. **Understand Deploy** - Is "Deploy" the sniping opportunity?
4. **Find Real Strategy** - What's the actual MEV/sniping angle?

## Possibilities

### Option A: Grok Was Wrong
The grid sniping strategy doesn't exist - Ore is traditional mining

### Option B: Different Ore Version
There might be Ore V1 vs V2, or a fork with grid system

### Option C: Deploy Sniping
The "Deploy" instruction (claim board space) might be the real opportunity

### Option D: Theoretical Future Feature
Grok described a planned but not implemented feature

## What to Do NOW

**STOP DEVELOPMENT** until we verify:
1. Does the Ore grid API exist? Test: curl https://ore.supply/v1/grid
2. What does the actual Ore program look like on-chain?
3. Is there a profitable sniping strategy for Ore Deploy?

**DO NOT GO LIVE** with current code - it may not work with real Ore protocol!

