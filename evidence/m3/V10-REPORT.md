# M3 V10 验证报告 — 中心备份/恢复（checkpoint 1+2）

- 日期：2026-11-16（Asia/Shanghai）
- 结论：**PASS** — 两个检查点（awaiting_review 在途审查、prompt_admitted 已领取在跑）均完成「备份→停中心→恢复→重启」全流程，恢复后既有运行按原 ID 续跑并正常收口，未发生重派；事件游标、请求/任务/run/证据均可按原 ID 核对。
- opencode 版本：v2.2.5
- 节点进程 PID（恢复后，本核验时刻）：central serve 38837；control 34004；worker-a 34007；worker-b 34012（opencode：33855/33904/33938）
- 探针任务：`req-18e874521f744057` / `task-18e8745500dc405d`（key `append-v10-lease-probe`，基线 `86ddc5d…`），目标：counter.py 末尾追加注释 `# v10 lease-continuity probe`。

## 检查点 2：审查在途（awaiting_review）时恢复

- 时间线：
  1. 14:51:24 备份 `central-backups/v10-bk-awaiting-145124`：此时 V09 两任务均 awaiting_review、两个审查 run 在途，事件游标 pre_max_seq=**98626**。
  2. 停止中心 → 从备份恢复 db → 重启中心。
  3. 恢复后两个审查 run 自动续完并提交 pass，两任务 accepted；post_max_seq=**98887**（增量 +261 条事件全部落在新库且有序）。
- 结果：审查通道在恢复后无一次失败或重派，最终判定 pass/pass。

## 检查点 1：run 已领取执行中（prompt_admitted）时恢复

- 时间线：
  1. 探针任务派发后，捕捉执行 run `run-18e8745502cfcd9b` 处于 `prompt_admitted`（租约剩余约 113s）时备份 `central-backups/v10-bk-claimed-145926`（15:00 前后），pre_max_seq=**98911**。
  2. 停中心 → 恢复 db → 重启中心。
  3. 该 run **未重派新 run**，按原 run_id 继续执行并 finished，产出候选 `066c63071ce8dc87cd735ae6720556d9ff6ee8b5`（提交信息"在 counter.py 末尾追加注释 # v10 lease-continuity probe"，会话 `ses_e1cd8c200ffelbV1QLYmuwW04n`，worker-b 4098）。
  4. 其审查 run `run-18e87463c0ab9d0c`（会话 `ses_e1cd7baacffeDhypbqKf32LGhe`）恢复正常租约续期，fixed_exit=0，verdict pass。
  5. 16:01（本地）复核：task/request 状态 **accepted**，`request.accepted` 事件 seq=**98959**。
- 结果：租约内被恢复的运行不丢工作、不重复执行。

## 通用判据核验（按原 ID 回查）

1. **ID 连续性**：requests（`req-18e873d33a426865`、`req-18e873d3b3e64ffc`、`req-18e874521f744057`）、tasks（`task-18e873d694b12962`、`task-18e873d8b881d70c`、`task-18e8745500dc405d`）、6 个 runs 全部可按原 ID 从恢复后的库查出，状态一致（3 accepted / finished）。
2. **事件流连续**：按原 entity_id 可拉取事件流；关键 seq（98626→98887、98911、98959）与登记观察值吻合；当前全局 max seq=99100。
3. **证据哈希复核**：3 个 execute run × 2 份证据（git.log、patch.diff）通过下载端点重算 sha256，与 evidence 表登记 **6/6 一致**：
   - `run-18e873d6cd8cb99d`：`75afb19f…`（patch）、`dd1d5824…`（log）
   - `run-18e873d92480485e`：`33ec2ace…`（patch）、`894b5e2d…`（log）
   - `run-18e8745502cfcd9b`：`cef4764b…`（patch）、`ee0a5bbb…`（log）
4. **无重派**：三任务均为 attempt=1 的单 execute + 单 review run，runs 表无因恢复产生的新 run。

## 恢复脚本缺陷观察（记录在案）

- `scripts/restore.sh` 末尾以 `(setsid nohup … serve &)` 启动中心，导致父脚本 do_wait 挂住不退出；且启动后 `sleep 4` 即 curl 探活过早，会误报 "central FAILED to start"（实际中心随后正常起来）。
- 本次实操采用 `timeout 60 bash scripts/restore.sh` 外层超时 + 事后独立 curl 核验的方式绕过；脚本本身待修复（低优先）。
- 备份/恢复期间 worker 与 control 侧 opencode 会话不参与恢复，会话历史保留在 opencode 进程内，中心库恢复后按 run 记录续接。

## 备注

- master 仍为 `86ddc5d…`（判据不要求合并候选）；V08 候选此前已合入。
- 两检查点备份目录保留于 `central-backups/`，证据文件副本存于 `evidence/m3/verify-*`。
