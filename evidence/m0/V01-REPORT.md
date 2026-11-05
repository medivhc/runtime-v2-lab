# V01 验证报告 · 三节点 OpenCode V2 接入（M0）

- 执行时间: 2026-11-06 08:00–08:10 (+08:00)
- 执行者: Worker 子代理 (TASK-001)
- 二进制: /root/workspace/.local/opencode/opencode, 版本 2.2.5,
  sha256 前 16 位 8d060e1d2fbb9bb4（全文见 pids.txt）
- 脚本: scripts/probe-oc.sh（附录 A 的非交互参数化版，断言逐条对应）

## 判定：PASS

## 逐项证据

| 附录 A 断言 | control | worker-a | worker-b |
|---|---|---|---|
| /api/info 版本为字符串 | 2.2.5 | 2.2.5 | 2.2.5 |
| 会话工作目录绑定 | PASS | PASS | PASS |
| FIRST-ACK 真实模型回包 | PASS | PASS | PASS |
| SECOND-<nonce> 恰一次（上下文+去重） | PASS | PASS | PASS |
| 消息 id2 在历史恰一条 | PASS | PASS | PASS |
| inbox 为空 | PASS | PASS | PASS |

探针会话 ID（可复查）:
- control:  ses_e51db442bffeo4gtz6hr2k2zXr
- worker-a: ses_e51db2bfaffez9iR9d172d0gaO
- worker-b: ses_e51db17b3ffeGFaW1TyqTSkrOE

## V01 通过判据核对

1. 三个服务 PID 不同: 5863 / 6458 / 6551 → PASS（pids.txt, PIDS-DISTINCT）
2. 原会话可读取历史且相同输入只出现一次:
   history.json 中 SECOND-<nonce> 长度恰为 1，id2 恰一条 → PASS
3. 异节点会话返回 404:
   worker-a→control 会话 404、control→worker-a 404、control→worker-b 404
   （isolation.txt）→ PASS

## 证据文件清单

- evidence/m0/<node>/{info,session,admitted-1,admitted-2,replay-2,history,user-message-2,inbox,wait-1,wait-2}.json
- evidence/m0/pids.txt、evidence/m0/isolation.txt

## 说明与限制

- 模型网关经 vps SSH 隧道 (127.0.0.1:13425→10.144.144.3:3425)，探针期间均可达。
- scripts/recover-oc.sh 对网关的健康检查未带认证头会误报 DOWN（已实测带 key
  返回 200）；不影响探针结论，待 M1 修正探测逻辑。
- 重复 ID 的 replay-2.json 返回 200（幂等受理），去重效果以会话历史恰一条为准，
  与附录 A 断言口径一致。
