#!/bin/bash
# Continuous agent-flow runner for 16 hours

cd /Users/mark/sentinel/bill_forge

DURATION_HOURS=${1:-16}
DURATION_SECONDS=$((DURATION_HOURS * 3600))
START_TIME=$(date +%s)
END_TIME=$((START_TIME + DURATION_SECONDS))
CYCLE_COUNT=0

echo "🚀 Starting continuous agent loop"
echo "⏱️  Duration: ${DURATION_HOURS} hours"
echo "🕐 Start: $(date)"
echo "🕐 End:   $(date -r $END_TIME 2>/dev/null || date -d @$END_TIME 2>/dev/null || echo "in ${DURATION_HOURS} hours")"
echo "---"

while true; do
  CURRENT_TIME=$(date +%s)
  ELAPSED=$((CURRENT_TIME - START_TIME))
  REMAINING=$((END_TIME - CURRENT_TIME))

  # Check if time is up
  if [[ $CURRENT_TIME -ge $END_TIME ]]; then
    echo ""
    echo "⏰ Time limit reached (${DURATION_HOURS} hours)"
    echo "📊 Total cycles completed: $CYCLE_COUNT"
    echo "🕐 Ended: $(date)"
    exit 0
  fi

  CYCLE_COUNT=$((CYCLE_COUNT + 1))
  HOURS_REMAINING=$((REMAINING / 3600))
  MINS_REMAINING=$(((REMAINING % 3600) / 60))

  echo ""
  echo "════════════════════════════════════════"
  echo "🔄 CYCLE #$CYCLE_COUNT | $(date)"
  echo "⏳ Time remaining: ${HOURS_REMAINING}h ${MINS_REMAINING}m"
  echo "════════════════════════════════════════"

  # Run the agent flow
  ./agent-flow.sh
  EXIT_CODE=$?

  if [[ $EXIT_CODE -eq 0 ]]; then
    echo "✅ Cycle #$CYCLE_COUNT completed successfully"
  else
    echo "⚠️  Cycle #$CYCLE_COUNT exited with code $EXIT_CODE"
  fi

  # Brief pause between cycles
  echo "💤 Pausing 10 seconds before next cycle..."
  sleep 10
done
