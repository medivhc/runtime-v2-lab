#!/usr/bin/env bash
# probe-oc.sh — 附录 A 探针的非交互参数化版
# 用法: bash probe-oc.sh control|worker-a|worker-b
#   或: OC_URL=http://127.0.0.1:4096 OC_NODE=control bash probe-oc.sh
# 断言逻辑与附录 A 逐条对应；证据写入 evidence/m0/<node>/。
set -euo pipefail
LAB=/root/workspace/runtime-v2-lab
case "${1:-${OC_NODE:-}}" in
  control)  node=control;  port=4096 ;;
  worker-a) node=worker-a; port=4097 ;;
  worker-b) node=worker-b; port=4098 ;;
  *) echo 'Usage: bash probe-oc.sh control|worker-a|worker-b' >&2; exit 2 ;;
esac
OC=${OC_URL:-http://127.0.0.1:$port}
OC_PASS=$(cat "$LAB/secrets/oc-$node.pass")
test -n "$OC_PASS"
OUT="$LAB/evidence/m0/$node"
rm -rf "$OUT"; mkdir -p "$OUT/probe"
OUT=$(cd "$OUT" && pwd -P)

api() { curl -fsS --max-time 300 -u "opencode:$OC_PASS" "$@"; }
post() { api -H 'Content-Type: application/json' -X POST "$1" --data "$2"; }

# 1) 服务协议与版本
api "$OC/api/info" | tee "$OUT/info.json" | jq -e '.version | type == "string"' >/dev/null
# 2) 会话创建与工作目录绑定
post "$OC/api/session" "$(jq -nc --arg d "$OUT/probe" \
  '{title:"Runtime V2 probe",location:{directory:$d}}')" > "$OUT/session.json"
sid=$(jq -er '.data.id' "$OUT/session.json")
jq -e --arg d "$OUT/probe" '.data.location.directory == $d' "$OUT/session.json" >/dev/null

# 3) 真实模型回包 + 同会话上下文 + 重复消息 ID 去重
nonce=$(python3 -c 'import secrets; print(secrets.token_hex(8))')
id1="msg_$(python3 -c 'import uuid; print(uuid.uuid4().hex)')"
id2="msg_$(python3 -c 'import uuid; print(uuid.uuid4().hex)')"
p1=$(jq -nc --arg id "$id1" --arg n "$nonce" \
  '{id:$id,text:("记住此代号 "+$n+"。只回复 FIRST-ACK"),delivery:"queue",resume:true}')
p2=$(jq -nc --arg id "$id2" \
  '{id:$id,text:"只回复 SECOND- 加上第一条消息中的代号。",delivery:"queue",resume:true}')

post "$OC/api/session/$sid/prompt" "$p1" > "$OUT/admitted-1.json"
post "$OC/api/experimental/session/$sid/wait" '{}' > "$OUT/wait-1.json"
post "$OC/api/session/$sid/prompt" "$p2" > "$OUT/admitted-2.json"
post "$OC/api/experimental/session/$sid/wait" '{}' > "$OUT/wait-2.json"
post "$OC/api/session/$sid/prompt" "$p2" > "$OUT/replay-2.json"

# 等待第二条回复写入，兼容 wait 在调度启动前返回的情况。
for _ in $(seq 1 120); do
  api "$OC/api/session/$sid/message?limit=100" > "$OUT/history.json"
  if jq -e --arg expected "SECOND-$nonce" \
      '[.data[] | select(.type == "assistant") | .. | strings | select(. == $expected)] | length == 1' "$OUT/history.json" >/dev/null; then
    break
  fi
  sleep 1
done

# 4) 断言：上下文续接（SECOND-<nonce> 恰一次）、消息 id2 恰一条
jq -e --arg expected "SECOND-$nonce" \
  '[.data[] | select(.type == "assistant") | .. | strings | select(. == $expected)] | length == 1' "$OUT/history.json" >/dev/null
api "$OC/api/session/$sid/message/$id2" > "$OUT/user-message-2.json"
jq -e --arg id "$id2" '[.data[] | select(.id == $id)] | length == 1' \
  "$OUT/history.json" >/dev/null
# 5) 收件箱为空
api "$OC/api/session/$sid/inbox" > "$OUT/inbox.json"
jq -e '.data | length == 0' "$OUT/inbox.json" >/dev/null

echo "V2 probe passed, node=$node, session=$sid, evidence=$OUT"
