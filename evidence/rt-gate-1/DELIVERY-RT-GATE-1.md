# RT-GATE-1 交付说明

## 改动摘要
新增 scripts/gate.sh（产物契约校验 + fmt/clippy/test 三段机器门）、scripts/gate-selftest.sh（五场景自测）、docs/gate.md（用法与失败策略枚举说明）。

## 测试结果
- cargo fmt --check：通过
- cargo test --workspace：77/77 全绿（基线只增不减）
- gate-selftest.sh：SELFTEST: PASS（含契约全满足/缺产物/空产物/glob 未命中场景）
- clippy：main 基线漂移（既有 store crate 4 处 too-many-arguments，非本单引入），见 rx-draft

## 验证方式
机器门实际执行记录 + 独立审查会话（worker-b，APPROVE）
