#!/usr/bin/env bash
# restore.sh — 受控恢复：停中心 → 恢复数据库与文件 → 状态核对 → 重新开放领取
# 用法：restore.sh <备份目录>
# 恢复后先打印待处理 run 与回传队列核对信息；确认无误后由调用方重启 runner。
set -eu
LAB=/root/workspace/runtime-v2-lab
SRC="${1:?usage: restore.sh <backup-dir>}"
[ -f "$SRC/runtime.sqlite" ] || { echo "no runtime.sqlite in $SRC"; exit 1; }

echo "[restore] stopping central (claiming stays closed until核对完成) ..."
pkill -f "runtime serve --config" 2>/dev/null || true
sleep 2

echo "[restore] restoring db ..."
cp "$SRC/runtime.sqlite" "$LAB/central/runtime.sqlite"

echo "[restore] restoring evidence ..."
if [ -d "$SRC/evidence" ]; then
  mkdir -p "$LAB/central/evidence"
  cp -a "$SRC/evidence/." "$LAB/central/evidence/"
fi

echo "[restore] restoring git refs (verify only; bare repo is source of truth) ..."
git --git-dir="$LAB/fixture.git" for-each-ref > /tmp/refs-now.txt
diff -u "$SRC/git-refs.txt" /tmp/refs-now.txt >/dev/null 2>&1 && echo "  refs identical" || echo "  WARN: refs differ from backup"

echo "[restore] state check before reopening claims ..."
python3 - "$LAB/central/runtime.sqlite" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
print("  runs by state:")
for r in c.execute("SELECT state, COUNT(*) FROM runs GROUP BY state"): print("   ", r)
print("  pending return queue (messages unacknowledged):")
for r in c.execute("SELECT kind, COUNT(*) FROM messages WHERE acknowledged_at IS NULL GROUP BY kind"): print("   ", r)
print("  event cursor (max seq):", c.execute("SELECT MAX(seq) FROM events").fetchone()[0])
EOF

echo "[restore] restarting central ..."
# Use setsid to fully detach from script's process group; redirect I/O; run in background.
# The subshell exits immediately after launching the detached process, so script won't wait on it.
(cd "$LAB" && setsid nohup "$LAB/target/debug/runtime" serve --config "$LAB/configs/central.toml" >> "$LAB/logs/central.log" 2>&1 < /dev/null &)

# Wait for central to become ready with retries (200ms interval, max 30s)
echo "[restore] waiting for central to become ready ..."
READY=0
MAX_ATTEMPTS=150  # 30s / 0.2s = 150
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
  if curl -fsS -m 2 -H "Authorization: Bearer ${RUNTIME_OPERATOR_TOKEN:-op-token-e2e}" \
       http://127.0.0.1:18080/v1/requests >/dev/null 2>&1; then
    READY=1
    break
  fi
  ATTEMPT=$((ATTEMPT + 1))
  sleep 0.2
done

if [ $READY -eq 1 ]; then
  echo "[restore] central ready"
else
  echo "[restore] central FAILED to start within 30s" >&2
  exit 1
fi

echo "[restore] done — runners may now be restarted to resume claiming."