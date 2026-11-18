# 客观门禁脚本

本文档说明 `scripts/gate.sh` 的用法、产物契约格式、失败策略枚举及其与本仓库流水线约定的对应关系。

---

## 概述

`gate.sh` 是本仓库的"机器门"实现，将口头约定转化为可执行、可复现的门禁检查。它在仓库根目录运行，执行两类检查：

1. **产物契约校验**（必选）：解析任务书中的「## 产物契约」区块，校验所有必需产物存在且非空。
2. **三段检查**（可选，默认开启）：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --workspace`。

任一检查失败即判定 `GATE: FAIL`，退出码 1；全部通过则 `GATE: PASS`，退出码 0。

---

## 用法

```bash
# 完整门禁（产物契约 + 三段检查）
scripts/gate.sh --task <任务书路径>

# 仅产物契约校验（自测、CI 预检查等场景）
scripts/gate.sh --task <任务书路径> --contract-only

# 显示帮助
scripts/gate.sh -h
scripts/gate.sh --help
```

**参数：**

| 参数 | 必需 | 说明 |
|------|------|------|
| `--task <path>` | 是 | 任务书 Markdown 文件路径（相对或绝对） |
| `--contract-only` | 否 | 仅做产物契约校验，跳过 fmt/clippy/test |
| `-h, --help` | 否 | 显示帮助信息并退出 |

**输出格式：**

```
PASS: artifacts/file1.txt
PASS: artifacts/file2.md
PASS: fmt
PASS: clippy
PASS: test
GATE: PASS
```

或失败时：

```
PASS: artifacts/file1.txt
FAIL: artifacts/file2.md (存在但为空)
FAIL: clippy
GATE: FAIL
```

**退出码：**

- `0` — GATE PASS
- `1` — GATE FAIL
- `2` — 参数错误

---

## 产物契约格式

任务书中必须包含以下结构的 Markdown 区块：

```markdown
## 产物契约
- 必需产物：
  - path/to/artifact1
  - path/to/artifact2
  - path/glob/*.rs
```

**解析规则：**

1. 识别 `## 产物契约` 二级标题（前缀 `##` 后跟空格）。
2. 在该区块内寻找 `- 必需产物：` 或 `- 必需产物:` 列表项（冒号中英文均可）。
3. 之后连续的 `- ` 缩进列表项视为必需产物路径。
4. 每项为相对路径（相对仓库根目录），支持 shell glob 模式（`*`、`?`、`[...]` 等）。
5. 遇到非 `- ` 开头的行或下一个 `##` 标题，结束解析。
6. 行内 `#` 后内容视为注释被忽略。

**校验规则：**

- 精确路径：文件必须存在且大小 > 0 字节。
- glob 模式：必须至少展开到一个文件，且**所有**展开文件均需大小 > 0 字节。
- 缺失、为空、glob 未命中均记为 `FAIL`。

---

## 失败策略枚举

本仓库流水线约定以下失败策略字段，`gate.sh` 当前实现对应 **机制一（仓库侧机器门）** 的行为：

| 字段 | 取值 | 含义 |
|------|------|------|
| `on_failure` | `loop` | 原地重试，上限 3 次，超限转人工 |
| | `reject` | 直接拒绝，不重试，标记任务失败 |
| | `fail` | 记录失败但继续流程（软性门禁） |
| | `human` | 转人工裁决，阻塞等待 |
| `retry_strategy` | `targeted_fix` | 打回原会话仅改 findings 涉及处 |
| | `full_redo` | 完整重做整个任务 |

**当前约定（任务书 RT-GATE-1 指定）：**

```yaml
retry_strategy: targeted_fix
on_failure: loop
```

**三轮上限兜底：**

- `on_failure: loop` 配合 `retry_strategy: targeted_fix` 时，最多允许 **3 次** 原地重试。
- 第 4 次仍失败 → 自动升级为 `human`（人工介入），不再自动重试。
- 此计数在任务级别维护，由上层编排器（而非 `gate.sh` 本身）负责跟踪与执行。

---

## 与流水线约定的对应关系

| 流水线阶段 | 对应检查 | 失败策略 |
|------------|----------|----------|
| **提交前自测** | `gate.sh --contract-only` | 开发者自行修复 |
| **CI 机器门** | `gate.sh --task <task.md>`（全检查） | `on_failure: loop` + `retry_strategy: targeted_fix`，上限 3 轮 |
| **人工复核前** | `gate.sh --task <task.md>`（全检查） | 必须 `GATE: PASS` 才能进入人工复核 |

**注意：**

- `gate.sh` 本身**不实现重试逻辑**，只负责单次检查并返回结构化结果（逐项 PASS/FAIL + 最终 GATE 结果 + 退出码）。
- 重试、计数、升级人工由上层编排器（如 `runtime` 服务、CI workflow）根据 `on_failure` / `retry_strategy` 字段执行。
- 任务书中的「## 失败与重试策略」区块为声明式配置，供编排器读取；`gate.sh` 仅解析产物契约区块。

---

## 示例任务书片段

```markdown
# 任务书 RT-EXAMPLE-1

## 失败与重试策略
- retry_strategy: targeted_fix
- on_failure: loop

## 产物契约
- 必需产物：
  - scripts/new-script.sh
  - docs/design.md
  - crates/**/tests/*.rs
  - evidence/run-*.json
```

---

## 自测脚本

`scripts/gate-selftest.sh` 验证 `gate.sh --contract-only` 在以下场景的行为：

1. **契约全满足** — 所有产物存在且非空 → `GATE: PASS`
2. **缺产物** — 列表中某文件不存在 → `GATE: FAIL`
3. **空产物** — 文件存在但大小为 0 → `GATE: FAIL`
4. **glob 未命中** — 模式不匹配任何文件 → `GATE: FAIL`
5. **glob 匹配含空文件** — 至少一个展开文件为空 → `GATE: FAIL`

运行方式：

```bash
scripts/gate-selftest.sh
```

输出 `SELFTEST: PASS` 且退出码 0 表示自测通过。

---

## 实现细节

- **Shebang**：`#!/usr/bin/env bash`
- **严格模式**：`set -euo pipefail`
- **幂等性**：可重复执行，无副作用
- **依赖**：标准 bash、coreutils（`grep`、`sed`、`find` 等）、Cargo
- **路径解析**：相对仓库根目录（脚本所在目录的父目录）

---

## 变更记录

| 版本 | 日期 | 变更 |
|------|------|------|
| v1 | 2026-11-18 | 初始版本（RT-GATE-1 交付） |