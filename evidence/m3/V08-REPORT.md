# V08 复跑报告（2026-11-16）

## 概述

V08（路线验证）复跑于 11-16 11:20 批次完成闭环：两任务（fix-counter-return、v08-count-root-files）执行→独立审查→accepted，候选已合入 fixture master。复跑过程中**定位并修复了排队卡死与运行搁浅的两个系统性根因**。

## 复跑前诊断（根因定位）

11-15 起 task-18e7ecf0a9950f3c 持续 queued、claim 均 204。11-16 批次诊断确认两个叠加缺陷：

1. **路线观测过期阻断派发**：`evaluate_claim` 要求任务 required_routes 对认领节点"存在、归属本人、未过期"。`lab-service` 的两条观测分别过期 112.7h / 37.8h，且 worker 心跳不刷新观测 → 全部认领被 `route_expired` 排除 → 任务永远 queued、claim 204。
2. **租约设计缺陷导致运行搁浅**：认领租约硬编码 30s（store claim `now()+30`），而 runner 的等模型续期间隔为 60s，且域规则禁止对已过期租约续期/推进 → 任何超过 30s 的运行必然停在 prompt_admitted（11-14 复跑 4 次 attempt 均因此 canceled/搁浅）。沙箱重置清掉本地 runner 状态后，孤运行无法恢复，任务卡 running。

## 修复

- `PUT /v1/resources/lab-service/observation` 以 worker-a/worker-b 令牌刷新观测（valid_seconds=86400），派发立即恢复。
- 代码修复（commit 见下）：认领租约 30s→120s；续期间隔 60s→20s。重编译并重启后，孤运行以 operator resolve(close) 裁决关闭、任务 retry 重派。
- 复验结论：fix-counter-return 与 v08-count-root-files 全链路（claim→session→prompt→finish→review→accepted）一次通过，长会话期间租约持续续期成功。

## 复跑记录（规格 §9 要求字段）

- OpenCode 版本：opencode v2.2.5；Runtime 二进制：target/debug/runtime（含租约修复）。
- 节点 PID（复跑时）：central 2076、control 2082、worker-a 2088、worker-b 2097；opencode 4096/4097/4098。
- 请求与任务：
  - req-18e773ac369ea042 / task-18e773ae07c84acf（fix-counter-return）→ accepted
  - req-18e7ecee71ef79b1 / task-18e7ecf0a9950f3c（v08-count-root-files）→ accepted
- 执行运行：
  - run-18e869c6b2f452ad（worker-b，attempt 6，session ses_e1d89d20bffe3LLcs7m1V32Zx6）候选 8459eb0ccdce3303b8133200be6a1126c8293b7a
  - run-18e869c6b63d09ec（worker-a，attempt 2，session ses_e1d89d16fffebYYyVHdl64bv2c）候选 86ddc5d88816ce574aafaf4a64ff9e2bc1fe2324
- 独立审查（control）：
  - run-18e869d00a24aa59（session ses_e1d891c8cffequDjdM6OsiUGkL）：fixed_exit=0，agent_verdict=pass，final=pass
  - run-18e86a07e5902c2d（session ses_e1d8571c0ffeHzdceb0XdTFrYL）：fixed_exit=0，agent_verdict=pass，final=pass
- 合入：fixture master fast-forward 至 86ddc5d（8459eb0 内容相同，diff 均仅 counter.py 一行）。
- 编排方机器门复验：基线 test_counter.py + 候选 counter.py，`python3 -m unittest` OK；`merge-base --is-ancestor` 通过；`diff --name-only` 仅 counter.py。
- v08-count-root-files 会话交付：根目录文件数=2（counter.py、test_counter.py），并按 counter-v1 验收档完成 counter 修复。
- 证据：evidence/m3/export-1116/{run-*/patch.diff, run-*/git.log}（经 /v1/runs/:id/evidence/<urlencoded rel_path> 下载，sha256 与中心登记一致可核对）。

## V08 通过判据核对

| 判据 | 结果 |
|---|---|
| 节点只能查到自己有权使用的路线 | PASS：worker-a 视图仅见 source_node=worker-a 的路线，worker-b 仅见自身，operator 视图为空 |
| A 的路线过期后被排除 | PASS：两节点观测分别过期 112.7h/37.8h 时，claim 均 204，调度器以 route_expired 排除（复跑前实测）；刷新观测后立即恢复派发 |
| 导出不包含合成凭据值 | PASS：导出文件全文扫描无 bearer/api key/password/token-e2e/vault:/内网地址等敏感串，仅见 credential_ref 引用名 |

## 备注

- 本轮为"现场诊断+修复"式复跑：路线过期由沙箱重置自然形成（非测试 fixture 注入），观测时间注入点留待后续验证加固。
- 发现的次要缺陷（待 V09/V10 前处理）：`runtime evidence` CLI 依赖不存在的 `GET /v1/runs/:id`（404 后静默导出空目录）；证据下载端点 `:name` 为单段参数，含斜杠的 rel_path 须 URL 编码（%2F）才能命中。
- 沙箱在批次执行后再次重置（12:31），已按恢复链重建全部服务并核验状态无损（任务/请求/合入结果均在）。
