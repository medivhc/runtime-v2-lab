# 任务书：evidence CLI 列表与下载路径修复

- 任务编号：RT-EVIDENCE-1（真实任务试金石 #2，源自 M3/V09-V10 实操缺陷）
- 仓库：runtime-v2-lab（evidence 导出 CLI，位于 crates/runtime 或其 bin 目标）
- 基线：main（派发时 HEAD 为准）

## 背景（实证记录，2026-11-16 M3 收官核验）

中心当前的路由表中不存在 `GET /v1/runs/:id`，但 evidence CLI 导出逻辑依赖
该端点，导致导出时静默产出空目录（不报错）。另外 `GET /v1/runs/:id/evidence/:name`
的 `:name` 是单段路径参数，CLI 拼接含斜杠的 rel_path（如
`run-xxx/patch.diff`）时未做 %2F 编码，直拼会 404。本轮核验时手工用
`urllib.parse.quote(rel, safe='')` 编码后下载成功，哈希 6/6 与登记一致，
证明服务端行为正确、缺陷全在 CLI 侧。

## 目标

1. CLI 改为不依赖不存在的端点：优先在中心新增 `GET /v1/runs/:id/evidence-list`
   （返回该 run 的 evidence 登记行），CLI 先取清单再逐个下载；若新增端点
   超出任务范围，允许改为 CLI 侧直接按已知文件名（patch.diff/git.log）下载，
   但必须在 DELIVERY 说明取舍。
2. 下载时对 `:name` 做 URL 编码（单段 %2F），并处理 404/哈希不一致时报错
   退出（非零码 + 明确错误信息），不再静默空导出。
3. 若新增端点：服务端实现须复用 store 层现有 evidence 查询，带鉴权与 404
   语义，路由测试覆盖。

## 验收档（acceptance）

- 中心如改代码：cargo fmt/clippy 干净，store/service 测试全绿（基线只增不减）。
- CLI 端到端：对 central 当前库中任一已有 run（如 run-18e8745502cfcd9b）
  导出成功且文件 sha256 与 evidence 表一致；对不存在的 run 导出返回非零码。
- 提交 diff 与报告符合现有 evidence 规范。

## 约束

- 不改动既有事件的写入路径与 evidence 表 schema。
- 不在本任务中修复 restore.sh（另单：RT-RESTORE-1）。
