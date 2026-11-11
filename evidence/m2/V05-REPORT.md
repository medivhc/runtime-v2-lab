# V05 验收报告：断点续跑（runner 暂停→原身份恢复）

- 判定：**PASS**（2026-11-09 线上验证）
- 规格：runner 用测试暂停点停在 V2 接收 prompt、中心尚未记入事件的位置；终止 runner，保持 V2 服务运行，按原身份重启后，同一个 run、会话和消息 ID 恢复，会话里只有一条对应输入。

## 实验设置

- 暂停配置：`configs/worker-a-v05-pause.toml`（`test_pause_after_prompt = true`，置于 `[repositories]` 之前——放文件末尾会被 TOML 当作嵌套键静默忽略）。
- 请求：`req-18e624439db7b95d`（2026-11-08T18:09:34Z 创建，任务 `task-18e62445159b20a8`，目标仓库 fixture，baseline `265f9fc`）。
- 执行节点：worker-a（opencode 4097）。

## 验证记录

| 项 | 值 |
| --- | --- |
| run ID | `run-18e624453cecb491`（attempt 1，暂停前后同一 ID） |
| 会话 ID | `ses_e43a5ea4bffeBmW0RSUQA0IF1X`（恢复后复用同一会话） |
| 用户消息 ID | `msg_de3644e21d264a64aa60e15071b79cd9`（恢复后无重复投递） |
| 会话用户消息数 | 1（`session_message` 中 `type='user'` 恰好 1 条，总消息 8 条） |
| 最终状态 | run finished；task **accepted** |

## 过程

1. 暂停配置启动 worker-a runner，prompt 投递后、`prompt_admitted` 事件落库前触发 `exit(0)`。
2. 保持 opencode V2 服务运行，仅终止 runner 进程。
3. 以原配置（无暂停开关）重启 runner：runner 以相同身份重连，认领恢复到同一 run，复用同一 opencode 会话与消息 ID 继续执行。
4. 断言：会话中该用户消息仅 1 条（jq 统计 `type=='user'` 计数 == 1），任务最终 accepted。

## 附带收获

暂停→复活的过程同时验证了恢复逻辑闭环（session_bound 容忍恢复、claim 失败不退出循环）。
