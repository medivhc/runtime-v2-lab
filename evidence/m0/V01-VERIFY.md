# V01 独立验证报告（TASK-002 · Verifier）

验证人：独立 Verifier 子代理，时间 2026-11-06 07:56–07:59。
方法：不依赖 Worker 脚本，自建 curl 断言独立重跑；先自行取证，后对照 Worker 报告。

## 独立重跑观测（evidence/m0/verify-1793923070/）

| 断言 | 结果 | 证据 |
|---|---|---|
| 三节点在线且网关可达（带认证头健康检查） | PASS | recover-oc.sh 输出 control=5863/worker-a=6458/worker-b=6551, gateway=up |
| 会话创建并绑定目录 | PASS | session.json location.directory 匹配 |
| FIRST-ACK 恰一条（assistant content.text 精确等值） | PASS | 计数=1（hist1.json） |
| SECOND-<nonce> 恰一条 | PASS | 计数=1（nonce=4dfa0c3acf9e815cb921） |
| 用户消息 id2 恰一条 | PASS | 计数=1 |
| 重放同 ID 同正文 SECOND 消息后仍各恰一条 | PASS | hist2.json 计数均=1，无新增逻辑输入 |
| worker-a 密码向 4097 查询 control 会话 | PASS | HTTP 404 |
| control 密码向 4096 查询 worker-a 证据会话 | PASS | HTTP 404 |
| Worker 证据完整性（三节点 info/session/history/inbox + pids.txt + isolation.txt） | PASS | 目录齐全；pids 互不相同；isolation 三组 404 |

## 验证方法说明
- 精确断言路径：`.data[]|select(.type=="assistant")|.content[].text` 精确等值。
  注意：直接对整个消息树做字符串聚合会把 reasoning 文本计入，导致虚高计数，
  Worker 脚本若采用聚合语义需以 content.text 为准复核（本次 Worker 结果与
  精确语义一致，见对照）。
- 重放检测使用与首次完全相同的 JSON 体（同消息 ID 同正文），符合规格幂等语义。

## 对照 Worker 自报
Worker 判定 PASS。独立重跑结论一致，无不符项。

## 最终判定
**V01 = PASS**（基于独立证据，非采信 Worker）。
