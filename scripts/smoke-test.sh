#!/usr/bin/env bash
# Kestrel smoke test: launch the real binary under Xvfb, let its built-in
# smoke mode open tabs + capture WebKit snapshots, then grab full-window
# screenshots with ImageMagick and assert basic health.
set -euo pipefail

BIN="${1:-target/release/kestrel}"
OUT="${2:-smoke}"
mkdir -p "$OUT"

echo "== Kestrel smoke test =="
"$BIN" --version

export DISPLAY=:99
# WebKit's bubblewrap sandbox cannot configure networking inside the
# restricted GitHub Actions container; disable it for CI evidence runs only
# (packaged product builds keep the sandbox enabled).
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
export GTK_A11Y=none
Xvfb :99 -screen 0 1440x900x24 &
XVFB_PID=$!
sleep 1.5

# Launch with the built-in smoke harness (opens tabs, snapshots, exits 0).
set +e
timeout 90 "$BIN" --smoke-test "$OUT" &
APP_PID=$!

# Give the UI time to map, then capture the real window contents.
sleep 12
import -window root "$OUT/window-full.png" 2>/dev/null || true
sleep 3
import -window root "$OUT/window-final.png" 2>/dev/null || true

wait "$APP_PID"
STATUS=$?
set -e

kill "$XVFB_PID" 2>/dev/null || true

echo "exit status: $STATUS"
if [ "$STATUS" -ne 0 ]; then
  echo "SMOKE TEST FAILED (exit $STATUS)"
  exit "$STATUS"
fi

# Assertions
if [ ! -f "$OUT/smoke-report.json" ]; then
  echo "MISSING smoke-report.json"; exit 4
fi
python3 - "$OUT" <<'EOF'
import json, sys, os
out = sys.argv[1]
r = json.load(open(os.path.join(out, "smoke-report.json")))
assert r.get("status") == "ok", f"report status: {r.get('status')}"
tabs = r.get("tabs", [])
assert len(tabs) >= 3, f"expected >=3 tabs, got {len(tabs)}"
assert r.get("filters_active") is True, "content filters not active"
print(f"SMOKE OK: {len(tabs)} tabs, engine_rules={r.get('engine_rules')}, tracker_hosts={r.get('tracker_hosts')}")
EOF

# PNG sanity: each snapshot must be a real render (non-trivial size).
for f in "$OUT"/shot-*.png; do
  [ -e "$f" ] || continue
  SIZE=$(stat -c%s "$f")
  echo "snapshot $f: $SIZE bytes"
  [ "$SIZE" -gt 2000 ] || { echo "snapshot too small: $f"; exit 5; }
done

echo "SMOKE TEST PASSED"
