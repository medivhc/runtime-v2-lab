# 任务书 RT-GATE-1：客观门禁脚本（机制一落地·仓库侧）

## 任务目标
把"机器门"从口头约定变成可执行脚本：新增 `scripts/gate.sh`，实现产物契约校验 + 三段检查（fmt/clippy/test），失败策略结构化。

## 失败与重试策略
- retry_strategy: `targeted_fix`（打回原会话只改 findings 涉及处）
- on_failure: `loop`（原地重试，上限 3 次，超限转人工）

## 产物契约（机器门校验存在性）
- 必需产物：
  - scripts/gate.sh
  - docs/gate.md
  - scripts/gate-selftest.sh

## 工作环境
- 工作树：/root/workspace/runtime-v2-lab/wt-gate-1（分支 rt/gate-1，基于 main=7c9bc22）
- 所有文件操作必须在该工作树内进行
- 仓库语言：Rust（crates/service、crates/runtime），附 scripts/、docs/、evidence/、configs/

## 功能需求
1. `scripts/gate.sh`（bash，set -euo pipefail）：
   - 用法：`gate.sh --task <任务书路径> [--contract-only]`，在仓库根目录运行。
   - 产物契约校验：解析任务书中「## 产物契约」区块下 `- 必需产物：` 之后的每个 `- ` 列表项（相对路径，支持 glob）；逐项校验存在且非空（glob 须至少展开到一个非空文件）。缺失或空 → 记 FAIL。
   - 三段检查（非 --contract-only 时）：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --workspace`，任一失败 → 记 FAIL。
   - 输出：逐项 `PASS/FAIL: <检查项>`，最后一行 `GATE: PASS` 或 `GATE: FAIL`；退出码 0/1。
2. `scripts/gate-selftest.sh`：自测脚本，用临时目录构造任务书与产物夹具（含：契约全满足、缺产物、空产物、glob 未命中四种场景），只调 `gate.sh --contract-only` 验证退出码与输出，全部断言真实比较，最后输出 SELFTEST: PASS/FAIL。
3. `docs/gate.md`：说明 gate.sh 用法、产物契约格式、失败策略枚举（on_failure: loop/reject/fail/human；retry_strategy: targeted_fix/full_redo）与 3 轮上限兜底，注明该文档与本仓库流水线约定的对应关系。

## 硬性约束
- 只允许新增/修改：scripts/gate.sh、scripts/gate-selftest.sh、docs/gate.md（验收档 gate-v1）
- 不得改动任何既有代码、脚本、配置；不改动 crates/
- 遵循现有代码风格与错误处理约定；脚本需可重复执行（幂等）

## 交付
- 在工作树内 git add 相关文件并 git commit（信息格式：`feat: add objective gate script with artifact contract (RT-GATE-1)`）
- 回复中列出：三个场景自测结果原文、gate.sh --help 输出
