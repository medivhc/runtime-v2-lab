#!/usr/bin/env bash
# start-oc.sh — 启动指定节点的 OpenCode V2 服务（规格附录 A 的自动化版）
# 用法: bash start-oc.sh control|worker-a|worker-b
set -euo pipefail
case "${1:-}" in
  control)  node=control;  port=4096 ;;
  worker-a) node=worker-a; port=4097 ;;
  worker-b) node=worker-b; port=4098 ;;
  *) echo 'Usage: bash start-oc.sh control|worker-a|worker-b' >&2; exit 2 ;;
esac
LAB=/root/workspace/runtime-v2-lab
root="$LAB/$node"
umask 077
mkdir -p "$root"/{data,state,cache,config/opencode,runner,work}
export XDG_DATA_HOME="$root/data"
export XDG_STATE_HOME="$root/state"
export XDG_CACHE_HOME="$root/cache"
export XDG_CONFIG_HOME="$root/config"
export OPENCODE_CONFIG_DIR="$root/config/opencode"
unset OPENCODE_SERVER_PASSWORD
case "$node" in
  control)  export OPENCODE_PASSWORD="$(cat $LAB/secrets/oc-control.pass)" ;;
  worker-a) export OPENCODE_PASSWORD="$(cat $LAB/secrets/oc-worker-a.pass)" ;;
  worker-b) export OPENCODE_PASSWORD="$(cat $LAB/secrets/oc-worker-b.pass)" ;;
esac
test -n "$OPENCODE_PASSWORD"
# 模型提供方配置：magpie 网关（经本地隧道），密钥由节点配置文件持有
if [ ! -f "$OPENCODE_CONFIG_DIR/opencode.json" ]; then
  KEY="$(jq -r .apiKey $LAB/secrets/magpie-provider.json)"
  MODELS="$(jq '.models' $LAB/secrets/magpie-provider.json)"
  jq -n --arg key "$KEY" --argjson models "$MODELS" '{
    "$schema": "https://opencode.ai/config.json",
    "model": "magpie/group/free",
    "provider": {
      "magpie": {
        "npm": "@ai-sdk/openai-compatible",
        "name": "Magpie Gateway",
        "options": { "baseURL": "http://127.0.0.1:13425/v1", "apiKey": $key },
        "models": $models
      }
    }
  }' > "$OPENCODE_CONFIG_DIR/opencode.json"
  chmod 600 "$OPENCODE_CONFIG_DIR/opencode.json"
fi
exec /root/workspace/.local/opencode/opencode serve --hostname 127.0.0.1 --port "$port"
