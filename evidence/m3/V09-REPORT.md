# M3 V09 验证报告 — 并发执行隔离

- 日期：2026-11-16（ Asia/Shanghai）
- 结论：**PASS** — 同一基线上并发提交的两个任务，全程会话/目录/候选/证据互相隔离，各自通过固定测试与独立审查后被接受，且每份审查报告只引用各自绑定的候选。
- opencode 版本：v2.2.5（`/root/workspace/.local/opencode/opencode`）
- 节点进程 PID（本核验时刻）：central serve 38837；control runner 34004；worker-a runner 34007；worker-b runner 34012（opencode serve：control 4096→33855、worker-a 4097→33904、worker-b 4098→33938）
- 基线 SHA：`86ddc5d88816ce574aafaf4a64ff9e2bc1fe2324`（master，未合入候选，判据不要求合并）

## 任务 A（worker-a 执行）

| 项 | 值 |
|---|---|
| 请求 / 任务 | `req-18e873d33a426865` / `task-18e873d694b12962` |
| 执行 run | `run-18e873d6cd8cb99d` |
| opencode 会话 | `ses_e1ce1053dffeLUQgcV4soS4zC2`（worker-a 4097） |
| 候选 commit | `69b9702d94caaf1dd698e9de4c29c1b3e5265924`（"Add v09-a marker comment to counter.py"） |
| 固定测试退出码 | 0 |
| 审查 run / 会话 | `run-18e873df12629e19` / `ses_e1ce06e1affejynJdVBijIlq4A`（prompt 消息 `msg_rev_run18e873df12629e19`） |
| 审查结果 | fixed_exit=0，agent_verdict=pass，final_verdict=pass |
| 任务最终状态 | **accepted**（request 同为 accepted） |
| 证据（中心登记 sha256） | git.log=`dd1d5824eae391a686b558694e42a3b3a5247cdddd2165219d19f49e52e9f298`；patch.diff=`75afb19f8a72f91a90f7a0eaa16f12e95a123c376d0bf47374a658561128c1ac` |

## 任务 B（worker-b 执行）

| 项 | 值 |
|---|---|
| 请求 / 任务 | `req-18e873d3b3e64ffc` / `task-18e873d8b881d70c` |
| 执行 run | `run-18e873d92480485e` |
| opencode 会话 | `ses_e1ce0df44ffeqeFKt77oTfLf7h`（worker-b 4098） |
| 候选 commit | `876e5669dfd381bbbb26ce0fbfb09d4ecf507141`（"Append v09-b marker comment to counter.py"） |
| 固定测试退出码 | 0 |
| 审查 run / 会话 | `run-18e873e1dbf6d41c` / `ses_e1cdec577ffehC8qCmE41U2Con` |
| 审查结果 | fixed_exit=0，agent_verdict=pass，final_verdict=pass |
| 任务最终状态 | **accepted**（request 同为 accepted） |
| 证据（中心登记 sha256） | git.log=`894b5e2d4a39e0135b8293c811f6036f72c01282059729be28413e4973ece084`；patch.diff=`33ec2ace9e4c57950e8c284d3c8277e884fe7d295c7dd585583503ab497a2e99` |

## 隔离性核验（判据逐条）

1. **会话隔离**：A、B 使用不同 opencode 实例（4097 vs 4098）与不同会话 ID，互不共享。
2. **目录隔离**：`worker-a/work/run-18e873d6cd8cb99d/checkout` 与 `worker-b/work/run-18e873d92480485e/checkout` 均独立存在。
3. **候选隔离**：两个候选 commit SHA 不同（`69b9702…` vs `876e566…`），且均基于同一基线 `86ddc5d…`；本核验再次以 `git rev-parse HEAD` 确认工作目录 HEAD 与登记一致。
4. **证据隔离**：4 份证据文件（每任务 git.log+patch.diff）sha256 全部不同；本轮通过下载端点 `GET /v1/runs/:id/evidence/:name`（name 需 `%2F` 编码）重算哈希，与中心 evidence 表登记值 **6/6 全部一致**（含 V10 探针，见 V10 报告）。
5. **审查绑定与报告引用隔离**：reviews 表中每条审查记录的 candidate_sha 与其被审 run 绑定正确。逐字核读两份审查会话助手输出：审查 A 全文 5 处引用 `69b9702…`、0 处提及 `876e566…`；审查 B 全文 2 处引用 `876e566…`、0 处提及 `69b9702…`。两份报告 verdict 均为 `{"verdict":"pass"}`，无交叉引用。

## 备注

- 每任务仅 1 次 execute + 1 次 review（attempt=1），无重派。
- 已知次要缺陷（不阻断）：runtime evidence CLI 依赖不存在的 `GET /v1/runs/:id` 会静默导出空目录；证据下载 name 参数需单段 `%2F` 编码。
- 过程中曾发生一次沙箱恢复（14:23 前后），核验在恢复后的同一套进程上完成，ID 均可按原值回查。
