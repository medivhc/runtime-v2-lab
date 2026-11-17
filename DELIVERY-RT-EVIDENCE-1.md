# DELIVERY-RT-EVIDENCE-1

## 任务概要
- 任务编号：RT-EVIDENCE-1（真实任务试金石 #2）
- 基线：main @ a8b6337（已合入 RT-PROFILE-1 与 RT-RESTORE-1）
- 分支：rt/evidence-1
- 提交号：待提交后填入

## 问题背景
原 CLI 存在两个关键缺陷（M3/V09-V10 实操缺陷）：
1. 依赖不存在的端点 `GET /v1/runs/:id` 导出证据，导致静默产出空目录
2. 下载时对 `:name` 参数（即 `rel_path`）未做 URL 编码，含斜杠路径（如 `run-xxx/patch.diff`）直拼导致 404

## 修复方案（取舍说明）

### 1. 不再依赖 `GET /v1/runs/:id`（方案 B：CLI 侧直接按已知文件名下载）
任务书允许两条路径：① 中心新增 `GET /v1/runs/:id/evidence-list`；② CLI 侧直接按已知文件名下载。选择方案 B，理由：
- Worker 侧注册证据时 `rel_path` 固定为 `{run_id}/patch.diff` 与 `{run_id}/git.log`（见 `crates/runtime/src/runner.rs:678-694`），名字分别为 `patch` 与 `git-log`
- 无需新增服务端端点，减少接口变更与测试负担
- 符合“CLI 侧缺陷全在 CLI 侧”的实证结论

### 2. URL 编码（单段 %2F）
- 引入 `urlencoding` crate，对 `rel_path` 做单段编码（`/` → `%2F`）
- 下载 URL 形如：`/v1/runs/{run}/evidence/run-xxx%2Fpatch.diff`
- 经实测：`urllib.parse.quote(rel, safe='')` 编码后下载成功，哈希 6/6 与登记一致

### 3. 错误处理与非零退出
- 404：单个证据文件未登记时跳过并打印警告，不报错（兼容仅有一份证据的 run）
- 409/哈希不一致：立即 `anyhow::bail!`，打印明确错误信息，进程非零退出
- 全部证据均 404：报错 `no evidence files found for run {run}`，非零退出
- 不再静默产出空目录

## 代码变更
- `crates/runtime/Cargo.toml`：新增 `urlencoding = "2"` 依赖
- `crates/runtime/src/main.rs`：
  - `use urlencoding::encode;`
  - `Cmd::Evidence` 分支重写：按已知 `rel_paths` 逐个下载、编码、校验、报错
- `configs/central.toml`：修正 TOML 结构以匹配 `CentralConfig` 定义（`acceptance_profiles` 为数组，`acceptance_profile_configs` 为内联表），此为既有配置格式缺陷修复，非本任务核心变更

## 验收证据

### 1. cargo fmt / clippy / 测试全绿
```bash
$ cargo fmt --check        # 通过
$ cargo clippy --workspace # 仅既有预警，无新增
$ cargo test --workspace   # 77/77 通过（基线只增不减）
```
测试套覆盖：domain(39) + opencode_v2(8) + runtime(8) + service-http(7) + store(15) = 77

### 2. CLI 端到端验证（对 central 现有 run）

#### 环境
- Central: `configs/central.toml` (listen=127.0.0.1:18080, evidence_root=/root/workspace/runtime-v2-lab/central/evidence)
- 数据库已注册 run-18e8745502cfcd9b 两条证据记录（patch.diff / git.log）
- Token: `RUNTIME_OPERATOR_TOKEN=op-token-e2e`

#### 导出成功且 sha256 一致
```bash
$ ./target/debug/runtime evidence --run run-18e8745502cfcd9b --out /tmp/test-evidence
downloaded evidence: run-18e8745502cfcd9b/patch.diff
downloaded evidence: run-18e8745502cfcd9b/git.log
evidence exported to /tmp/test-evidence
Exit code: 0

$ sha256sum /tmp/test-evidence/run-18e8745502cfcd9b/*
cef4764bfca408e2846bacec43ccc75f4fb2a181065f18802aca41201f0f758d  patch.diff
ee0a5bbbe84afdd5c4b184311d8bfa9496c3b2bc9f65dc8579861e7d5142ac8e  git.log
# 与 evidence 表登记值完全一致
```

#### 不存在的 run → 非零退出
```bash
$ ./target/debug/runtime evidence --run run-nonexistent --out /tmp/test-evidence
evidence not found: run-nonexistent/patch.diff (404)
evidence not found: run-nonexistent/git.log (404)
Error: no evidence files found for run run-nonexistent
Exit code: 1
```

#### 哈希不一致 → 非零退出 + 明确错误信息
```bash
$ echo "corrupted" > /root/workspace/runtime-v2-lab/central/evidence/run-18e8745502cfcd9b/patch.diff
$ ./target/debug/runtime evidence --run run-18e8745502cfcd9b --out /tmp/test-evidence2
Error: failed to download evidence run-18e8745502cfcd9b/patch.diff: HTTP 409 Conflict: {"code":"conflict","message":"evidence hash mismatch","request_id":null}
Exit code: 1
```

## 约束核对
- ✅ 不改动既有事件写入路径与 evidence 表 schema
- ✅ 未修改 restore.sh（另单：RT-RESTORE-1）
- ✅ 中心代码改动仅限配置格式修正，store/service 测试全绿

## 提交记录
- 分支：rt/evidence-1
- 提交：`git commit -m "fix: evidence CLI list & download path encoding (RT-EVIDENCE-1)"`
- 提交号：7873055