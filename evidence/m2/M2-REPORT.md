# M2 验收总结报告（本地多节点：V04–V07）

- 判定：**M2 通过**（V04–V07 全部 PASS）
- 日期：V04 2026-11-09；V05 2026-11-09；V06 2026-11-09 晚；V07 2026-11-11
- 规格：`docs/RUNTIME_OPENCODE_V2_PROJECT_SPEC.md` M2 行——worker-b、按能力调度、租约恢复、消息去重，V04–V07 通过。

## 各项结论

| 项 | 主题 | 判定 | 报告 |
| --- | --- | --- | --- |
| V04 | 双 worker 并发抢领，仅一胜，错误租约代际 409 | PASS | 见 git c353117 提交说明与屏障测试 |
| V05 | runner 暂停→原身份恢复，run/会话/消息 ID 不变，会话仅一条输入 | PASS | [V05-REPORT.md](V05-REPORT.md) |
| V06 | control 暂停→结果冲刷恰好一条，跨节点越权被拒 | PASS | [V06-REPORT.md](V06-REPORT.md) |
| V07 | 按能力派单 + 节点离线等待 + 恢复完成 | PASS | [V07-REPORT.md](V07-REPORT.md) |

## 测试与验证口径

- 单元/集成：全量测试绿（store 15、service http 8、domain 7、runtime 14，2026-11-11 复跑）。
- 线上端到端：真实 opencode V2 会话 + 中心 Runtime（127.0.0.1:18080），每次复跑记录请求/任务/运行/会话/消息 ID 与最终判定（见各分报告）。
- 受控实验纪律：先停 `serve-all.sh` 与 `recover-watchdog.sh`，防止旧配置进程被复活干扰；TOML 顶层测试开关必须置于 `[repositories]` 之前。

## M2 期间修复（git）

- `8763edd`：planner prompt 重试使用后缀消息 ID（失败轮次不可重投）；`RefCellConn` → 阻塞 Mutex（并发处理 panic）；serve-all.sh dash 兼容。
- `d4bf2b0`：评审通过后同事务自动收尾请求（`accept_request_if_complete` 此前从未被调用）。
- 本次（V07）：`resolve_run(close)` 回写任务状态修复 + 回归测试（详见 V07-REPORT）。

## 环境备注

- 沙箱多次重置均由 `/root/workspace/.local/bootstrap-sandbox.sh` 一键恢复（Rust 工具链持久化于 `.local/{rustup,cargo}`，clang 按需 apt 补装，mesh/opencode/Runtime 全链路拉起）。
- 模型网关经 vps 隧道 `-L 13425`（magpie/group/free）。

## 遗留与下一步

- V06 暂停点造成的评审运行行遗留已裁决关闭（见 V06-REPORT 遗留说明）；冲刷路径可考虑在投递成功后同事务推进运行行，作为后续加固项。
- M3（下一里程碑）待启动。
