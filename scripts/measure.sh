#!/usr/bin/env bash
# Measure RAM + CPU of a running Kestrel instance under Xvfb.
# Produces a CSV timeline and a JSON summary (peak/idle values).
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
Xvfb :99 -screen 0 1440x900x24 &
sleep 1.5

"$BIN" --smoke-test /tmp/kestrel-metrics-smoke &
APP_PID=$!
echo "kestrel pid: $APP_PID"

echo "t_seconds,kestrel_rss_kb,webkit_rss_kb,total_rss_kb,cpu_percent" > "$OUTCSV"

collect() {
  local t=$1
  local k_rss=0 w_rss=0 cpu=0
  # kestrel main process
  if [ -r "/proc/$APP_PID/status" ]; then
    k_rss=$(awk '/VmRSS/ {print $2}' "/proc/$APP_PID/status" 2>/dev/null || echo 0)
  fi
  # WebKit child processes (WebProcess / NetworkProcess)
  for p in $(pgrep -P "$APP_PID" 2>/dev/null; pgrep -f "WebKitWebProcess|WebKitNetworkProcess" 2>/dev/null); do
    [ -r "/proc/$p/status" ] || continue
    # only count processes whose parent chain reaches our app
    local rss
    rss=$(awk '/VmRSS/ {print $2}' "/proc/$p/status" 2>/dev/null || echo 0)
    w_rss=$((w_rss + rss))
  done
  # CPU% of the app process from /proc
  if [ -r "/proc/$APP_PID/stat" ]; then
    read -r utime stime <<< "$(awk '{print $14, $15}' "/proc/$APP_PID/stat")"
    cpu=$(( (utime + stime) ))
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
peak = max(col("total_rss_kb"))
idle = col("total_rss_kb")[-1]
summary = {
    "samples": len(rows),
    "peak_total_rss_mb": round(peak / 1024, 1),
    "final_total_rss_mb": round(idle / 1024, 1),
    "timeline_head": rows[:3],
}
open(sys.argv[1].replace(".csv", "-summary.json"), "w").write(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
EOF
