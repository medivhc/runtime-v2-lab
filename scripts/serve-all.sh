#!/usr/bin/env bash
# serve-all.sh — 拉起中心 + 双 runner，若进程缺失自动补（由 exec setsid 常驻调用）
set -u
LAB=/root/workspace/runtime-v2-lab
. $LAB/scripts/env.sh
# 令牌以 secrets/runtime.env 为准；e2e 兜底值仅在缺失时使用。
if [ -f $LAB/secrets/runtime.env ]; then
  set -a; . $LAB/secrets/runtime.env; set +a
fi
export RUNTIME_OPERATOR_TOKEN=${RUNTIME_OPERATOR_TOKEN:-op-token-e2e} RUNTIME_CONTROL_TOKEN=${RUNTIME_CONTROL_TOKEN:-ctl-token-e2e} RUNTIME_WORKER_A_TOKEN=${RUNTIME_WORKER_A_TOKEN:-wa-token-e2e} RUNTIME_WORKER_B_TOKEN=${RUNTIME_WORKER_B_TOKEN:-wb-token-e2e}
export OC_CONTROL_PASSWORD="$(cat $LAB/secrets/oc-control.pass)"
export OC_WORKER_A_PASSWORD="$(cat $LAB/secrets/oc-worker-a.pass)"
export OC_WORKER_B_PASSWORD="$(cat $LAB/secrets/oc-worker-b.pass)"

start_missing() {
  pgrep -f "runtime serve --config" >/dev/null || \
    (cd $LAB && setsid nohup $LAB/target/debug/runtime serve --config $LAB/configs/central.toml >> $LAB/logs/central.log 2>&1 &)
  pgrep -f "runtime runner --config .*control.toml" >/dev/null || \
    (cd $LAB && setsid nohup $LAB/target/debug/runtime runner --config $LAB/configs/control.toml >> $LAB/logs/control-runner.log 2>&1 &)
  pgrep -f "runtime runner --config .*worker-a.toml" >/dev/null || \
    (cd $LAB && setsid nohup $LAB/target/debug/runtime runner --config $LAB/configs/worker-a.toml >> $LAB/logs/worker-a-runner.log 2>&1 &)
  pgrep -f "runtime runner --config .*worker-b.toml" >/dev/null || \
    (cd $LAB && setsid nohup $LAB/target/debug/runtime runner --config $LAB/configs/worker-b.toml >> $LAB/logs/worker-b-runner.log 2>&1 &)
}

start_missing
sleep 4
while true; do
  sleep 20
  start_missing
done
