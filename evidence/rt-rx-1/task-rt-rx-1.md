# 任务书 RT-RX-1：归档处方草稿脚本（机制三落地·仓库侧）

## 任务目标
给归档环节加一步"处方草稿"：新增 `scripts/rx-draft.sh`，从任务书与 evidence 归档（审查结论、DELIVERY）自动提取 0–2 条改进建议，动作严格限定枚举，只生成草稿、不自动应用。

## 失败与重试策略
- retry_strategy: `targeted_fix`（打回原会话只改 findings 涉及处）
- on_failure: `loop`（原地重试，上限 3 次，超限转人工）

## 产物契约（机器门校验存在性）
- 必需产物：
  - scripts/rx-draft.sh
  - docs/rx.md
  - scripts/rx-selftest.sh

## 工作环境
- 工作树：/root/workspace/runtime-v2-lab/wt-rx-1（分支 rt/rx-1，基于 main=7c9bc22）
- 所有文件操作必须在该工作树内进行
- 仓库语言：Rust（crates/service、crates/runtime），附 scripts/、docs/、evidence/、configs/

## 功能需求
1. `scripts/rx-draft.sh`（bash，set -euo pipefail）：
   - 用法：`rx-draft.sh --task <任务书路径> --evidence <evidence目录> --out <输出路径>`
   - 输入读取：任务书全文；evidence 目录下的 `review-*-verdict.txt`（含 FINDINGS 行）与 `DELIVERY*.md`。
   - 提取规则：从审查结论的每条 finding（critical/major/minor 行）与 DELIVERY 中声明的返工/遗留记录，归纳出 0–2 条处方建议；无 findings 且无遗留 → 输出"无处方建议"。
   - 处方动作枚举（三选一，禁止其他动作）：`update_task_template`（改任务书模板）/ `add_gate_check`（加机器门检查项）/ `append_review_prompt`（追加审查提示词）。
   - 输出 rx-draft.md：每条处方含 `action:`（枚举值）、`rationale:`（引用依据原文行）、`status: DRAFT`；文件头注明"处方草稿，须人工确认后应用，脚本不自动修改任何流程文件"。
2. `scripts/rx-selftest.sh`：自测脚本，用临时目录构造夹具（含：有 findings 生成 1–2 条、无 findings 输出无处方、非法动作被拒绝三种场景），断言真实比较输出与退出码，最后输出 SELFTEST: PASS/FAIL。
3. `docs/rx.md`：说明处方草稿的定位（借鉴 LoopForge runeval 三段式：事实提取→受控处方→人工应用）、动作枚举语义、人工确认流程（确认后由编排方写入任务书模板或审查提示词，脚本本身永不写流程文件）。

## 硬性约束
- 只允许新增/修改：scripts/rx-draft.sh、scripts/rx-selftest.sh、docs/rx.md（验收档 rx-v1）
- 不得改动任何既有代码、脚本、配置；不改动 crates/；脚本不得写任务书、审查提示词等流程文件
- 遵循现有代码风格与错误处理约定；脚本需可重复执行（幂等）

## 交付
- 在工作树内 git add 相关文件并 git commit（信息格式：`feat: add prescription draft generator for archive step (RT-RX-1)`）
- 回复中列出：三场景自测结果原文、示例 rx-draft.md 内容
