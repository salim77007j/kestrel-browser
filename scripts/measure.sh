#!/usr/bin/env bash
# Measure RAM + CPU of a running Kestrel instance under Xvfb.
# Launches a PLAIN browser window (no --smoke-test: that mode self-exits
# after capturing evidence, which would zero every later sample).
# Produces a CSV timeline and a JSON summary (peak / idle values).
set -euo pipefail

BIN="${1:-target/release/kestrel}"
OUTCSV="${2:-smoke/metrics.csv}"
mkdir -p "$(dirname "$OUTCSV")"

export DISPLAY=:99
# WebKit's bubblewrap sandbox cannot configure networking inside the
# restricted GitHub Actions container; disable it for CI evidence runs only
# (packaged product builds keep the sandbox enabled).
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
export GTK_A11Y=none
pkill -f "Xvfb :99" 2>/dev/null || true
sleep 0.5
Xvfb :99 -screen 0 1440x900x24 &
sleep 1.5

"$BIN" >/tmp/kestrel-measure.log 2>&1 &
APP_PID=$!
echo "kestrel pid: $APP_PID"

echo "t_seconds,kestrel_rss_kb,webkit_rss_kb,total_rss_kb,cpu_percent" > "$OUTCSV"

PREV_J=""

collect() {
  local t=$1
  local k_rss=0 w_rss=0 cpu=0
  # kestrel main process
  if [ -r "/proc/$APP_PID/status" ]; then
    k_rss=$(awk '/VmRSS/ {print $2}' "/proc/$APP_PID/status" 2>/dev/null || echo 0)
  fi
  # WebKit child processes of THIS instance only (WebProcess / NetworkProcess
  # are direct children; no pgrep -f → no pollution from earlier runs).
  local p rss
  for p in $(pgrep -P "$APP_PID" 2>/dev/null); do
    [ -r "/proc/$p/status" ] || continue
    rss=$(awk '/VmRSS/ {print $2}' "/proc/$p/status" 2>/dev/null || echo 0)
    w_rss=$((w_rss + rss))
  done
  # CPU% of the app process since the previous sample
  # (utime+stime in jiffies; CLK_TCK=100 → percent = delta_jiffies / 2s)
  if [ -r "/proc/$APP_PID/stat" ]; then
    local cur_j
    cur_j=$(awk '{print $14 + $15}' "/proc/$APP_PID/stat")
    if [ -n "$PREV_J" ]; then
      cpu=$(awk -v c="$cur_j" -v p="$PREV_J" 'BEGIN { printf "%.0f", (c - p) / 2 }')
    fi
    PREV_J=$cur_j
  fi
  echo "$t,$k_rss,$w_rss,$((k_rss + w_rss)),$cpu" >> "$OUTCSV"
}

for t in $(seq 2 2 30); do
  sleep 2
  collect "$t"
done

kill "$APP_PID" 2>/dev/null || true
sleep 1
pkill -f "Xvfb :99" 2>/dev/null || true

python3 - "$OUTCSV" <<'EOF'
import csv, sys, json
rows = list(csv.DictReader(open(sys.argv[1])))
if not rows:
    print(json.dumps({"error": "no samples"})); sys.exit(1)
def col(name): return [int(r[name]) for r in rows]
# App is alive for every sample → last row is the true steady-state (idle).
peak = max(col("total_rss_kb"))
idle = col("total_rss_kb")[-1]
cpu_avg = round(sum(col("cpu_percent")) / len(col("cpu_percent")), 1)
summary = {
    "samples": len(rows),
    "peak_total_rss_mb": round(peak / 1024, 1),
    "idle_total_rss_mb": round(idle / 1024, 1),
    "cpu_avg_percent": cpu_avg,
    "timeline_head": rows[:3],
    "timeline_tail": rows[-3:],
}
open(sys.argv[1].replace(".csv", "-summary.json"), "w").write(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
EOF