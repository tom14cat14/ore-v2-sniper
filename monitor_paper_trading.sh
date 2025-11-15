#!/bin/bash
# ORE Bot - Paper Trading Monitor
# Shows execution stats in real-time

LOG_FILE="/tmp/ore_paper_trading.log"
PID_FILE="/tmp/ore_bot.pid"

echo "════════════════════════════════════════════════════════"
echo "  🎯 ORE BOT - PAPER TRADING MONITOR"
echo "════════════════════════════════════════════════════════"
echo ""

# Check if bot is running
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if ps -p $PID > /dev/null 2>&1; then
        echo "✅ Bot Status: RUNNING (PID: $PID)"
    else
        echo "❌ Bot Status: NOT RUNNING"
        echo "   Start with: cd /home/tom14cat14/ORE && ./start_paper_trading.sh"
        exit 1
    fi
else
    echo "❌ Bot Status: NOT RUNNING (no PID file)"
    exit 1
fi

echo ""
echo "════════════════════════════════════════════════════════"
echo "  📊 EXECUTION STATS"
echo "════════════════════════════════════════════════════════"

# Count executions
TOTAL_EXECUTIONS=$(grep -c "MULTI-CELL PORTFOLIO" "$LOG_FILE" 2>/dev/null || echo "0")
PAPER_TRADES=$(grep -c "PAPER TRADE: Would deploy" "$LOG_FILE" 2>/dev/null || echo "0")
NO_OPPORTUNITY=$(grep -c "No opportunity" "$LOG_FILE" 2>/dev/null || echo "0")

# Runtime
START_TIME=$(head -1 "$LOG_FILE" | cut -d' ' -f1 | cut -dT -f2 | cut -d. -f1)
CURRENT_TIME=$(date -u +%H:%M:%S)
echo "⏱️  Runtime: Started at $START_TIME UTC (now: $CURRENT_TIME UTC)"
echo ""
echo "📈 Opportunities Found: $TOTAL_EXECUTIONS"
echo "✅ Paper Trades: $PAPER_TRADES"
echo "⚠️  Skipped (no +EV): $NO_OPPORTUNITY"
echo ""

# Last 3 executions
echo "════════════════════════════════════════════════════════"
echo "  🎯 LAST 3 EXECUTIONS"
echo "════════════════════════════════════════════════════════"
grep -A 3 "MULTI-CELL PORTFOLIO" "$LOG_FILE" | tail -16 | grep -E "(MULTI-CELL|Cell.*EV:)" | while read line; do
    if echo "$line" | grep -q "MULTI-CELL"; then
        echo ""
        echo "$line" | sed 's/.*MULTI-CELL/🎯 MULTI-CELL/'
    else
        echo "$line" | sed 's/.*Cell /   Cell /'
    fi
done

echo ""
echo "════════════════════════════════════════════════════════"
echo "  📡 LIVE STREAM (Ctrl+C to exit)"
echo "════════════════════════════════════════════════════════"
echo ""

# Live tail filtered for important events
tail -f "$LOG_FILE" | grep --line-buffered -E "(MULTI-CELL|PAPER TRADE|opportunity|until snipe.*0\.)" | while read line; do
    timestamp=$(echo "$line" | cut -d' ' -f1 | cut -dT -f2 | cut -d. -f1)
    message=$(echo "$line" | sed 's/.*INFO //')
    echo "[$timestamp] $message"
done
