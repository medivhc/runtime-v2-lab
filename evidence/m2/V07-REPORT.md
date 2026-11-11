# V07 验收报告：按能力派单 + 节点离线等待

- 判定：**PASS**（2026-11-11 线上验证）
- 规格：提交仅 worker-b 满足的 `fixture-special` 任务，完成后停掉 B，等待心跳过期，再提交同类任务并恢复 B；两次执行均在 B，等待期无其他节点启动，恢复后有验收和原会话确认记录。

## 环境与配置

- worker-a 能力 `["fixture-exec"]`；worker-b 能力 `["fixture-exec","fixture-special"]`（`configs/worker-b.toml`，opencode 4098）。
- 离线阈值：`offline_after_seconds = 60`（`configs/central.toml`）。
- 目标仓库 fixture（bare，counter.py + test_counter.py，baseline `265f9fc`）。

## 第一阶段：能力路由 + 中断恢复

- 请求 `req-18e662fb497b9ccb`（`--require fixture-special`，2026-11-09T13:18:52Z 提交）。
- attempt 1：`run-18e662fc84162041` 仅被 worker-b 认领（能力路由生效：worker-a 缺 `fixture-special` 不会接）。执行中途沙箱重置，运行行滞留 running、租约过期。
- 处置：操作员 `resolve --decision close` 关闭旧运行 → `retry` 重排任务 → attempt 2 `run-18e6f2d1c75a5ebf` 仍由 worker-b 执行完成 → 评审 `run-18e6f2d69cc0399b`（control）通过 → 请求 **accepted**。

### 过程中暴露并修复的缺陷

`resolve_run(close)` 只推进运行行、不回写任务状态，任务卡在 running 导致重试报非法转换（`illegal transition: task running -> queued`）。

修复（提交见 M2-REPORT）：`resolve_run` close 分支同事务把 running 任务置为 canceled 并记录 reason，与 `advance_run` 的失败路径对齐；新增回归测试 `resolve_close_from_running_reopens_task_for_retry`，store 测试 15/15 通过。

## 第二阶段：离线等待实验（2026-11-11）

受控前提：停 `serve-all.sh`（监管循环）、`recover-watchdog.sh`（自愈）与 worker-b runner；保留 central、control、worker-a 在线。

1. 停 B 后提交同类任务：请求 `req-18e6f2ee3f2c1b71`（`--require fixture-special`，任务 `task-18e6f2ef1c813315`）。
2. 等待心跳过期后核对（2026-11-11T09:18Z 前后）：

| 观测项 | 值 |
| --- | --- |
| worker-b 心跳 | `1794388600`，观测时刻 `1794388697`，**过期 97 秒 > 60 秒阈值 → 离线** |
| worker-a 心跳 | `1794388695`（2 秒前，在线且持续轮询） |
| 任务状态 | `queued`，运行数 **0**（等待期无任何节点认领） |

3. 恢复 B：worker-b 重启后立即认领 `run-18e6f30460746565` → 执行 finished → 评审 `run-18e6f3083969c353`（control）finished → 请求 **accepted**。

## 结论

- 两次执行均在 worker-b（attempt 1/2 与离线实验 run 各一次），worker-a 全程未认领能力专属任务。
- 等待期（B 离线 97 秒）无其他节点启动执行。
- 恢复后有验收（accepted）与原会话确认记录（评审与结果消息走原规划会话，见 V06-REPORT 的会话口径）。
