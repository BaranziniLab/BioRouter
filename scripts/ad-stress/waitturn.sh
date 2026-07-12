#!/usr/bin/env bash
# Wait until the served-app daemon completes one more agent turn (a new "done" frame).
LOG=/tmp/ad-stress-daemon.log
n0=$(grep -c '"type":"done"' "$LOG" 2>/dev/null); case "$n0" in ''|*[!0-9]*) n0=0;; esac
for i in $(seq 1 "${1:-50}"); do
  n1=$(grep -c '"type":"done"' "$LOG" 2>/dev/null); case "$n1" in ''|*[!0-9]*) n1=0;; esac
  [ "$n1" -gt "$n0" ] && { echo "turn settled ($n0->$n1)"; exit 0; }
  sleep 3
done
echo "timeout"
