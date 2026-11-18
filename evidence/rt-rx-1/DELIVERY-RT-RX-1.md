# RT-RX-1 交付说明

## 改动摘要
新增 scripts/rx-draft.sh（从任务书与 evidence 归档生成 0–2 条处方草稿，动作限定三选一枚举）、scripts/rx-selftest.sh（六场景自测）、docs/rx.md（三段式定位与人工确认流程说明）。

## 测试结果
- cargo fmt --check：通过
- cargo test --workspace：77/77 全绿
- rx-selftest.sh：6/6 场景 PASS（有 findings 生成/无 findings 输出无处方/非法动作拒绝）

## 验证方式
机器门实际执行记录 + 独立审查会话（worker-b，APPROVE 零 findings）
