#!/bin/bash
# Test Deploy transaction execution
# This script manually triggers a Deploy to test the skip_preflight fix

cd /home/tom14cat14/ORE

echo "🧪 Testing Deploy Transaction Execution"
echo "========================================="
echo ""
echo "Configuration:"
echo "  Wallet: 8MBg94RS4WTPbggpkAUbsxauqq5HfL5DEvRn8rGcQB7u"
echo "  Skip Preflight: ENABLED (fix applied)"
echo "  Force Test Mode: ENABLED"
echo ""
echo "📝 Note: This will attempt to deploy SOL to the current round"
echo "         If all cells are already deployed, it will fail gracefully"
echo ""
echo "Press Ctrl+C to cancel, or wait 3 seconds to continue..."
sleep 3
echo ""
echo "🚀 Executing test deployment..."
echo ""

# Set environment to force immediate execution
export RUST_LOG=info
export FORCE_TEST_MODE=true
export EXECUTE_ONCE_AND_EXIT=true
export PAPER_TRADING=false
export ENABLE_REAL_TRADING=true

# Run the bot
timeout 60 ./target/release/ore_sniper 2>&1 | tee /tmp/deploy_test.log

echo ""
echo "========================================="
echo "📊 Test Complete"
echo ""
echo "Check logs above for transaction results"
echo "Full log saved to: /tmp/deploy_test.log"
