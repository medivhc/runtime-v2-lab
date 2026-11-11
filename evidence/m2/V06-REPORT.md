# V06 验收报告：结果投递去重（control 暂停→冲刷→恰好一条）

- 判定：**PASS**（2026-11-09 晚全链路验证；2026-11-11 复核）
- 规格：使 control 的投递循环暂停，重复提交同一验收结果，再恢复投递；只有一条逻辑消息，恢复后进入原会话，越权读取或确认被拒绝。

## 实验设置

- 暂停配置：`configs/control-v06-pause.toml`（`test_pause_before_result = true`，置于 `[repositories]` 之前）。暂停点位于「评审通过、结果回传规划会话之前」：写 `control/runner/pending_result.json` 后 `exit(0)`。
- 受控前提：先停 `serve-all.sh` 与 `recover-watchdog.sh`，避免旧配置进程被复活干扰实验。
- 实验请求：`req-18e6627021c474a1`（2026-11-09T13:08:55Z 创建，任务 `task-18e662726e1cdaab`，执行 run `run-18e66272addddfeb` @ worker-a，评审 run `run-18e66277b85b73af` @ control）。
- 规划会话：`ses_e51b36757ffezSxQCJvkY9R5g7`（control 节点 opencode 4096）。

## 验证记录

| 步骤 | 断言 | 结果 |
| --- | --- | --- |
| 暂停触发 | `pending_result.json` 落盘，control 进程退出 | ✓ |
| 冲刷前 | 规划会话中无 `msg_result_run18e66277b85b73af` | ✓（0 条） |
| 恢复投递 | 正常配置重启 control，冲刷待投递结果，文件被消费删除 | ✓ |
| 冲刷后 | 该结果消息恰好 **1** 条（稳定消息 ID `msg_result_<review_run>` 去重） | ✓ |
| 请求收尾 | 评审通过后同事务自动 accepted（`request.accepted` 2026-11-09T13:10:01Z） | ✓ |
| 跨节点隔离 | worker 凭据访问 control 节点会话 API 被拒（401）；本节点凭据正常（200） | ✓ |

## 去重口径

规划会话 `ses_e51b36757ffezSxQCJvkY9R5g7` 共 19 条 `msg_result_*` 消息，每条 ID 唯一、对应各自的评审 run，无重复投递（2026-11-11 复核）。

## 遗留说明

暂停点位于「评审通过、请求 accepted」之后、「评审 run 状态推进 + 结果投递」之前，因此该评审 run 行停留在 running。结果消息已由冲刷路径按稳定 ID 投递恰好一次，请求已 accepted；该运行行已于 2026-11-11 由操作员裁决关闭（`run.resolved_closed`，reason 注明 V06 实验遗留）。
