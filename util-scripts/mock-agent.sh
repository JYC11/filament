#!/usr/bin/env bash
# Mock agent subprocess for testing filament dispatch.
# Reads env vars to control behavior:
#   MOCK_AGENT_STATUS    - AgentResult status (default: "completed")
#   MOCK_AGENT_SUMMARY   - AgentResult summary (default: "mock agent done")
#   MOCK_AGENT_DELAY_MS  - Delay before output in milliseconds (default: 0)
#   MOCK_AGENT_EXIT_CODE - Process exit code (default: 0)
#   MOCK_AGENT_MESSAGES  - JSON array of messages (default: "[]")
#   MOCK_AGENT_NOISE     - If set, print noise lines before JSON
set -euo pipefail

STATUS="${MOCK_AGENT_STATUS:-completed}"
SUMMARY="${MOCK_AGENT_SUMMARY:-mock agent done}"
DELAY_MS="${MOCK_AGENT_DELAY_MS:-0}"
EXIT_CODE="${MOCK_AGENT_EXIT_CODE:-0}"
MESSAGES="${MOCK_AGENT_MESSAGES:-[]}"

# Optional delay
if [ "$DELAY_MS" -gt 0 ]; then
    sleep "$(echo "scale=3; $DELAY_MS/1000" | bc)"
fi

# Optional noise before JSON
if [ -n "${MOCK_AGENT_NOISE:-}" ]; then
    echo "Starting mock agent..."
    echo "Processing task..."
fi

# Output AgentResult JSON
cat <<EOF
{"status":"${STATUS}","summary":"${SUMMARY}","artifacts":[],"messages":${MESSAGES},"blockers":[],"questions":[]}
EOF

exit "$EXIT_CODE"
