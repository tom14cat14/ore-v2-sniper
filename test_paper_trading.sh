#!/bin/bash
# Simple paper trading test

echo "🎯 Starting Ore Sniper Paper Trading Test"
echo "=========================================="
echo ""

export PAPER_TRADING=true
export ENABLE_REAL_TRADING=false
export USE_SHREDSTREAM_TIMING=false
export RUST_LOG=info

echo "Configuration:"
echo "  PAPER_TRADING: true"
echo "  ENABLE_REAL_TRADING: false"  
echo "  USE_SHREDSTREAM_TIMING: false"
echo ""
echo "Running for 30 seconds..."
echo ""

timeout 30 ./target/release/ore_sniper 2>&1 | tee /tmp/ore_paper_test.log

echo ""
echo "=========================================="
echo "Test completed. Log saved to /tmp/ore_paper_test.log"

