#!/bin/bash
# ORE Sniper Auto-Restart Script with Loop Protection
# Location: /home/tom14cat14/ORE/start_ore_sniper.sh

cd /home/tom14cat14/ORE

# Loop protection settings
MAX_RESTARTS=10
RESTART_WINDOW=300  # 5 minutes
MIN_RUNTIME=30      # Minimum seconds before counting as crash

# Track restarts
RESTART_COUNT=0
WINDOW_START=$(date +%s)

# Clean environment (avoid config conflicts)
unset JITO_MAX_TIP_LAMPORTS MAX_CONSECUTIVE_FAILURES MIN_PROFIT_SOL \
      MAX_DAILY_TRADES MAX_DETECTION_AGE_SECONDS MAX_CONCURRENT_OPPORTUNITIES \
      MAX_LOSS_SOL

echo "========================================"
echo "ORE Sniper Starting"
echo "Time: $(date)"
echo "Loop protection: $MAX_RESTARTS restarts per ${RESTART_WINDOW}s"
echo "========================================"

while true; do
    # Check if we've exceeded restart limit in window
    NOW=$(date +%s)
    ELAPSED=$((NOW - WINDOW_START))

    if [ $ELAPSED -gt $RESTART_WINDOW ]; then
        # Reset window
        RESTART_COUNT=0
        WINDOW_START=$NOW
        echo "[$(date)] Restart window reset"
    fi

    if [ $RESTART_COUNT -ge $MAX_RESTARTS ]; then
        echo "========================================"
        echo "[$(date)] LOOP PROTECTION TRIGGERED!"
        echo "Too many restarts ($RESTART_COUNT) in ${RESTART_WINDOW}s window"
        echo "Bot stopped to prevent crash loop"
        echo "Check logs and fix issue before restarting"
        echo "========================================"
        exit 1
    fi

    # Start the bot
    START_TIME=$(date +%s)
    LOG_FILE="logs/ore_sniper_$(date +%Y%m%d_%H%M%S).log"

    echo "[$(date)] Starting ORE Sniper (restart #$RESTART_COUNT)..."
    echo "[$(date)] Log file: $LOG_FILE"

    # Run the bot
    ./target/release/ore_sniper 2>&1 | tee "$LOG_FILE"
    EXIT_CODE=$?

    END_TIME=$(date +%s)
    RUNTIME=$((END_TIME - START_TIME))

    echo ""
    echo "[$(date)] Bot exited with code $EXIT_CODE after ${RUNTIME}s"

    # Only count as crash if runtime was very short
    if [ $RUNTIME -lt $MIN_RUNTIME ]; then
        RESTART_COUNT=$((RESTART_COUNT + 1))
        echo "[$(date)] Quick crash detected (${RUNTIME}s < ${MIN_RUNTIME}s)"
        echo "[$(date)] Restart count: $RESTART_COUNT / $MAX_RESTARTS"
    else
        # Healthy exit or long runtime, reset counter
        RESTART_COUNT=0
        WINDOW_START=$(date +%s)
        echo "[$(date)] Normal exit after ${RUNTIME}s, restart count reset"
    fi

    echo "[$(date)] Restarting in 5 seconds..."
    sleep 5
done
