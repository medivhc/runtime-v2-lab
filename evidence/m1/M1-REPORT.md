# M1 REPORT — 里程碑 M1（单任务闭环）总结

日期：2026-11-07（北京时间）。结论：**M1 完成（V01–V03 全部 PASS）**。

## 交付范围
- **V01（M0，独立复核 PASS）**：三节点探针，证据 `evidence/m0/`。
- **V02（端到端闭环）**：见 `V02-REPORT.md`。请求 → 计划 → 执行 → 评审 → 接受，
  三个独立会话真实模型轮次，候选 SHA/证据哈希可核，结果回传规划会话。
- **V03（对抗核验）**：见 `V03-REPORT.md`。基线/篡改测试/真实修复三路，
  前两路被固定检查驳回（退出码 1），仅真实修复通过（退出码 0）。

## 代码资产（分支 main）
- crates：`domain`（状态机/调度规则）、`store`（SQLite 存储）、
  `service`（HTTP 中心）、`runtime`（runner：worker 执行 + control 规划/评审）、
  `opencode_v2`（V2 API 适配器）。
- CLI 子命令：`serve / runner / submit / status / events / evidence / resolve / retry`。
- 测试：全量 67 项通过（domain 39、opencode_v2 8、runtime 7、service 13）。

## 本阶段修复清单（git log 见提交）
1. NFS `.nfs` silly-rename EBUSY → checkout 原位复用（fetch+checkout --force+reset+clean）。
2. `refs/runs/*` 显式 refspec fetch，候选 SHA 可解析。
3. attempt 间 message_id/session 隔离（仅同 run 复用），消除 opencode 409。
4. 所有 git 调用固定 GIT_DIR/GIT_WORK_TREE，杜绝向上发现外层仓库。
5. review claim 查询补 session_id 列（列类型错误修复）。
6. 子代理等待超时加长（执行 1800s / 评审 900s / 规划回执 600s），
   等待期间每 60s 续租；中心租约 30s→300s，离线窗口 15s→60s。
7. serve-all.sh 令牌来源修正（secrets/runtime.env 优先）且仅用 bash 调用。

## 遗留事项
- runner 评审失败后 review run 停留 running，无自动回收（需 operator 重置或
  M2 引入租约过期回收机制——规格 M2/V04 范畴）。
- control 规划循环对已接受请求重复提交计划（422 刷日志，不影响任务流，待 M2 修）。
- M2（V04–V07）与 M3（V08–V10）未开始。
