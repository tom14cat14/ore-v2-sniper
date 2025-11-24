#!/usr/bin/env python3
import sys
sys.path.insert(0, '/home/tom14cat14/sol-pulse.com/ml_bot')

from solders.pubkey import Pubkey
from solana.rpc.api import Client
import struct

# Connect to RPC
rpc = Client("https://edge.erpc.global?api-key=507c3fff-6dc7-4d6d-8915-596be560814f")

# Get Treasury PDA
ORE_PROGRAM = Pubkey.from_string("oreV3EG1i9BEgiAJ8b177Z2S2rMarzak4NMv1kULvWv")
TREASURY_SEED = b"treasury"

treasury_pda, bump = Pubkey.find_program_address([TREASURY_SEED], ORE_PROGRAM)
print(f"Treasury PDA: {treasury_pda}")

# Fetch account
account_info = rpc.get_account_info(treasury_pda)
data = account_info.value.data

print(f"\nAccount data length: {len(data)} bytes")
print(f"First 80 bytes (hex): {data[:80].hex()}")

# Try different offsets to find 238 ORE
target = int(238 * 1e11)  # 238 ORE with 11 decimals
print(f"\nLooking for value around: {target} (238 ORE with 11 decimals)")

for offset in range(0, min(len(data)-8, 100), 8):
    value = struct.unpack('<Q', data[offset:offset+8])[0]
    as_ore_9 = value / 1e9
    as_ore_11 = value / 1e11
    if 200 < as_ore_9 < 300 or 200 < as_ore_11 < 300:
        print(f"Offset {offset}: {value} = {as_ore_9:.2f} ORE (9 decimals) or {as_ore_11:.2f} ORE (11 decimals)")
