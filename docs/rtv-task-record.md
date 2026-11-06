# RTV 任务记录 · Runtime 多 Agent 协作系统

## 原始目标（权威，不可收窄）
按 `docs/RUNTIME_OPENCODE_V2_PROJECT_SPEC.md` 完整实现 Runtime 多 Agent 协作系统，
完成全部里程碑 M0–M3（非 demo、非 MVP），按 RTV 技能执行：所有实现任务由子代理完成，
协调者负责编排与独立验收；git 管理，每阶段提交；实验 agent 为 OpenCode V2；
模型使用 magpie 网关 `group/free`（与本流水线既有配置相同）。

## 约束
- 规格 §9 开发顺序与验收表为唯一退出判据；V01–V10 全部通过。
- 节点令牌/密码只落本地进程配置（secrets/，不入库不入模型上下文）。
- 模型调用全部走免费路由，出现计费立即停。
- 验证证据必须来自独立重跑，不采信 Worker 自报。

## 能力基线（Phase 0 已核实）
- opencode v2.2.5 官方 Linux 版（sha256 前 16 位 8d060e1d2fbb9bb4）装于
  /root/workspace/.local/opencode；V2 原生 API 自测通过（会话创建/队列 prompt/
  真实模型回包 FIRST-ACK，约 3 秒/轮）。
- magpie 网关经 vps SSH 隧道 127.0.0.1:13425 可达，key 已验证（/v1/models 与
  chat/completions 均通）；网关密钥文件 secrets/magpie-provider.json（0600）。
- Rust stable 工具链在 /root/workspace/.local/rustup（env.sh 已导出）。
- 后台进程会被轮次回收：所有常驻进程用 scripts/recover-oc.sh 恢复，验证前先跑。

## 任务树
- TASK-000 (协调者): 仓库/环境/密钥/文档基线 — DONE（本提交）
- TASK-001 (Worker): M0 — 三节点 opencode 探针与 V01 证据
  - 验收: 附录 A 探针对 4096/4097/4098 全过；三服务 PID 不同；原会话可续接、
    相同输入去重；异节点查询会话 404；证据文件落 evidence/m0/
- TASK-002 (Verifier): 独立复跑 V01 关键断言
- TASK-003 (Worker): M1 — 中心/CLI/control/worker-a/fixture/独立验收闭环（V02,V03）
- TASK-004 (Verifier): 独立复跑 V02/V03
- TASK-005 (Worker): M2 — worker-b/能力调度/租约恢复/消息去重（V04–V07）
- TASK-006 (Verifier): 独立复跑 V04–V07
- TASK-007 (Worker): M3 — 资源目录/证据导出/备份恢复/运维说明（V08–V10）
- TASK-008 (Verifier): 独立复跑 V08–V10 与全量回归
- TASK-009 (Auditor): 对照原始规格逐条审计覆盖度与证据缺口

## 失败分类与预算
按 RTV 默认：每类失败最多 2 次实质性修复尝试；分解深度 ≤3；证据不新增不重试。

## 进展日志
- TASK-003a：Worker 实现完成（domain/store/service/runtime，59 测试全绿），
  提交前会话被打断、子代理树被清空。协调者于 2026-11-06 独立复跑
  cargo test --workspace 全绿后代为提交；接口约定见 crates 代码与本文件。
