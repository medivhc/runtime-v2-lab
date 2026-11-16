# 任务书：restore.sh 收尾挂起与探活过早修复

- 任务编号：RT-RESTORE-1（真实任务试金石 #1，源自 M3/V10 实操缺陷）
- 仓库：runtime-v2-lab（scripts/restore.sh）
- 基线：main（85xx 起，以派发时 HEAD 为准）

## 背景（实证记录，2026-11-16 V10 检查点恢复实操）

`scripts/restore.sh` 恢复中心数据库并重启中心，但存在两个缺陷：
1. 脚本末尾以 `(setsid nohup ... serve &)` 启动中心后，脚本自身的 do_wait
   逻辑把 serve 子进程当作自己的子进程等待，导致脚本永不退出，调用方必须
   外层 `timeout 60` 强杀；
2. 启动后 `sleep 4` 即 curl 探活，中心此时往往未完成初始化，误报
   "central FAILED to start"（实际稍后能起来）。

## 目标

修复 restore.sh，使其：
1. 启动中心后脚本本身在合理时间内正常退出（serve 脱离脚本进程组，不阻塞 do_wait）；
2. 探活采用带重试的就绪等待（如 200ms 间隔、最长 30s、对固定健康检查端点成功即返回），就绪后打印 "central ready"；
3. 探活失败时明确报错并以非零码退出（不得误报成功，也不得把慢启动当失败）。

## 验收档（acceptance）

- 修改仅限 `scripts/restore.sh`（如需提取辅助函数，限 scripts/ 目录内新文件）。
- 恢复流程功能不回归：对当前 central-backups 中的任一 v10 备份目录跑通
  备份→恢复→重启链路（评审方可实际执行一次）。
- 自测：`bash -n scripts/restore.sh` 通过；模拟运行（可用本地桩验证探活重试逻辑，不必真重启中心，评审时说明桩点即可）。
- 提交 diff 与报告符合现有 evidence 规范（git.log + patch.diff）。

## 约束

- 不改动 central/worker 代码与数据库 schema。
- 不在本任务中修复 evidence CLI（另单：RT-EVIDENCE-1）。
