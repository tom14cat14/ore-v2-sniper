#!/bin/bash
# ORE Sniper Bot - Auto-restart with loop protection
# Restarts on crash, but stops if it crashes too fast (loop protection)

cd /home/tom14cat14/ORE
source .env

MIN_RUNTIME=30  # Minimum seconds bot should run before restart
MAX_FAST_RESTARTS=3  # Max restarts within MIN_RUNTIME before giving up
fast_restart_count=0
last_start_time=0

echo "🎯 ORE Sniper Bot Launcher"
echo "=========================="
echo "Mode: LIVE TRADING"
echo "Min EV: ${MIN_EV_PERCENTAGE}%"
echo "Snipe Window: ${SNIPE_WINDOW_SECONDS}s"
echo "Deploy per cell: ${DEPLOYMENT_PER_CELL_SOL} SOL"
echo ""

while true; do
    current_time=$(date +%s)

    # Check for fast restart loop
    if [ $last_start_time -ne 0 ]; then
        runtime=$((current_time - last_start_time))
        if [ $runtime -lt $MIN_RUNTIME ]; then
            fast_restart_count=$((fast_restart_count + 1))
            echo "⚠️  Fast restart detected (ran for ${runtime}s). Count: $fast_restart_count/$MAX_FAST_RESTARTS"

            if [ $fast_restart_count -ge $MAX_FAST_RESTARTS ]; then
                echo "❌ Too many fast restarts! Stopping to prevent loop."
                echo "   Check logs: tail -100 /tmp/ore_sniper.log"
                exit 1
            fi

            # Wait before restarting to prevent rapid loops
            echo "⏳ Waiting 10s before restart..."
            sleep 10
        else
            # Reset counter if bot ran long enough
            fast_restart_count=0
        fi
    fi

    last_start_time=$(date +%s)
    echo ""
    echo "🚀 Starting ORE Sniper at $(date)"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    ./target/release/ore_sniper 2>&1 | tee -a /tmp/ore_sniper.log

    exit_code=$?
    echo ""
    echo "⚠️  Bot exited with code $exit_code at $(date)"
    echo "🔄 Restarting in 5 seconds..."
    sleep 5
done
