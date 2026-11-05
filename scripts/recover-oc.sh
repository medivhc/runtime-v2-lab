#!/usr/bin/env bash
# recover-oc.sh — 恢复 runtime-v2-lab 全部常驻进程：网关隧道 + 三个 opencode 节点
# 幂等；每次验证/执行前先跑一遍。
set -u
LAB=/root/workspace/runtime-v2-lab
mkdir -p $LAB/logs

# 1) 网关隧道（经 vps）
if ! curl -fsS --max-time 3 http://127.0.0.1:13425/v1/models -o /dev/null 2>/dev/null; then
  pkill -f 'ssh -fN -L 13425' 2>/dev/null
  setsid nohup ssh -fN -L 13425:10.144.144.3:3425 -p 12224 -i /root/.ssh/id_mesh \
    -o StrictHostKeyChecking=no -o ServerAliveInterval=30 -o ServerAliveCountMax=3 \
    -o ExitOnForwardFailure=yes root@127.0.0.1 >> $LAB/logs/gw-tunnel.log 2>&1
  sleep 3
fi

# 2) 三个 opencode 节点
for node in control worker-a worker-b; do
  case "$node" in
    control) port=4096 ;; worker-a) port=4097 ;; worker-b) port=4098 ;;
  esac
  pass="$(cat $LAB/secrets/oc-$node.pass 2>/dev/null)"
  if ! curl -fsS --max-time 3 -u "opencode:$pass" http://127.0.0.1:$port/api/info -o /dev/null 2>/dev/null; then
    pkill -f "opencode serve --hostname 127.0.0.1 --port $port" 2>/dev/null
    setsid nohup bash $LAB/scripts/start-oc.sh $node >> $LAB/logs/oc-$node.log 2>&1 &
    sleep 5
  fi
done

# 3) 状态汇总
for spec in control:4096 worker-a:4097 worker-b:4098; do
  node=${spec%%:*}; port=${spec##*:}
  pass="$(cat $LAB/secrets/oc-$node.pass)"
  pid=$(curl -fsS --max-time 3 -u "opencode:$pass" http://127.0.0.1:$port/api/info 2>/dev/null | jq -r '.pid // "down"')
  echo "$node port=$port pid=$pid"
done
curl -fsS --max-time 3 http://127.0.0.1:13425/v1/models -o /dev/null 2>/dev/null && echo "gateway=up" || echo "gateway=DOWN"
