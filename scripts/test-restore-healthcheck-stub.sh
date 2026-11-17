#!/usr/bin/env bash
# Test stub for restore.sh health check retry logic
# This simulates the health check loop without starting actual central server

set -eu

# Simulate a server that becomes ready after N attempts
# Usage: test-restore-healthcheck-stub.sh <attempts_before_ready>
# If attempts_before_ready > MAX_ATTEMPTS, it simulates failure

ATTEMPTS_BEFORE_READY="${1:-5}"
MAX_ATTEMPTS=150
ATTEMPT=0

echo "[stub] Simulating health check: server ready after $ATTEMPTS_BEFORE_READY attempts"

# Mock curl function that simulates server becoming ready
mock_curl() {
  ATTEMPT=$((ATTEMPT + 1))
  if [ $ATTEMPT -ge $ATTEMPTS_BEFORE_READY ]; then
    echo "[stub] Attempt $ATTEMPT: HTTP 200 OK"
    return 0
  else
    echo "[stub] Attempt $ATTEMPT: connection refused / not ready"
    return 1
  fi
}

READY=0
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
  if mock_curl; then
    READY=1
    break
  fi
  sleep 0.01  # Fast sleep for testing
done

if [ $READY -eq 1 ]; then
  echo "[stub] central ready (after $ATTEMPT attempts)"
  exit 0
else
  echo "[stub] central FAILED to start within $MAX_ATTEMPTS attempts" >&2
  exit 1
fi