# V02 REPORT — 端到端真实闭环（规划 → 执行 → 验收）

日期：2026-11-07（北京时间）。结论：**PASS**。

## 验证目标（规格 V02）
一次任务在全系统中真实走完：规划、执行、验收三个独立会话真实产生；
候选 SHA 与证据可核；结果 ID 回传原规划会话。

## 执行链路（事件序号可核，中心 DB：central/runtime.sqlite）
- `req-18e54ddff107e832` 创建（seq 12），计划被 control 接受（seq 17，request.plan_accepted）。
- `task-18e54de435ca4e64` 执行运行 `run-18e5b60201a46412`（第 27 次尝试，此前失败见 V02b 备注）：
  - run.session_bound（seq 518，session `ses_e4adfd0f8ffeg2R4gO7Hj4gxT0`，worker-a 节点）
  - run.prompt_admitted（seq 519）→ run.running（seq 524）
  - evidence.added（seq 526/527）→ run.finished（seq 528）
- 评审运行 `run-18e5b607ae8ef613`（review.queued seq 529）由 control 领取，
  reviews 表记录：`fixed_check_exit_code=0`、`agent_verdict=pass`、`final_verdict=pass`。
- 任务终态 `task-18e54de435ca4e64 = accepted`。

## 独立会话核验（三个会话分属不同节点/角色）
| 角色 | 节点 | 会话 |
|---|---|---|
| 规划（planner） | control :4096 | `ses_e51b36757ffezSxQCJvkY9R5g7`（runtime planner） |
| 执行（worker） | worker-a :4097 | `ses_e4adfd0f8ffeg2R4gO7Hj4gxT0` |
| 验收（review） | control :4096 | `ses_e4a1032c7ffex1IJQjxzAszx0u`（runtime review run-18e5b607ae8ef613） |

评审会话为真实模型轮次：会话消息中包含对基线/候选的 diff 检查与
`{"verdict":"pass"}` 回复（opencode control 节点 history 可查）。

## 候选 SHA 与证据核验
- 候选：`2207edf0cd29f16ecc93173f3bea713c127f02d2`，从 fixture.git
  `refs/runs/run-18e5b60201a46412` 可解析（bare 仓库对象存在）。
- patch 证据：`central/evidence/run-18e5b60201a46412/patch.diff`
  - 本地文件 sha256 `e71f52a8…9ae759` 与 DB 登记值一致（evd-18e5b607a423bfd3）。
  - git.log 证据登记 evd-18e5b607a97a3c9c（sha256 `17f39557…8e8d0d`）。
- 内容：`45d1b4b Fix increment to return value + 1`（模型提交）+ `2207edf runtime: task execution`（runner 收尾提交）。

## 结果回传原规划会话
- 消息 `msg_result_run18e5b607ae8ef613` 已投递至规划会话
  `ses_e51b36757ffezSxQCJvkY9R5g7`（opencode 控制节点消息列表可见该 user 消息 ID）。

## 附注（稳定性缺陷修复记录）
第 1–26 次尝试失败的原因均已在本次修复（提交见 git log）：
- NFS 工作区 `.nfs` silly-rename 导致 `remove_dir_all` EBUSY → checkout 改为原位复用；
- `refs/runs/*` 未随 clone/fetch 拉取 → 显式 refspec；
- 旧 attempt 的 message_id 泄漏到新会话（opencode 409 Conflict）→ 仅同 run 复用；
- 子目录 git 命令向上发现外层 lab 仓库 → 全部调用固定 GIT_DIR/GIT_WORK_TREE；
- review 领取查询缺 session_id 列（rusqlite 列类型错误）→ 补列；
- 模型轮次长于租约 → 等待期间每 60s 续租，等待超时 600s→1800s（执行）/300s→900s（评审）。

## 判定
V02 判据全部满足：**PASS**。
