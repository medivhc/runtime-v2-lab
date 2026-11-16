# 任务书：验收档位参数化（真实任务接入前置）

- 任务编号：RT-PROFILE-1（真实任务试金石 #0，前置任务）
- 仓库：runtime-v2-lab（crates/runtime/src/runner.rs 审查流程 + configs）
- 基线：main（派发时 HEAD 为准）

## 背景（2026-11-16 代码核验）

流水线当前固定验收逻辑在 control 端硬编码：审查流程只认
「changed files == [counter.py] + 候选用 baseline 的 test_counter.py 跑
python3 -m unittest」（runner.rs process_reviews 约 685–750 行）。任何非
counter.py 修复的真实任务都无法通过现有审查门。repo_registry/central.toml
已有 acceptance_profiles 字段（现为名称列表 ["counter-v1"]），但并未映射到
可执行命令——档位名与行为脱钩。

## 目标

把固定验收参数化为按档位（acceptance profile）配置：

1. 配置格式：central.toml 的 `[repositories.<name>]` 支持档位定义，每档包含
   - `test_cmd`：固定测试命令（在审查工作目录内执行，如
     `bash scripts/acceptance/service-v1.sh`）；
   - `allowed_paths`：允许变更的文件/目录 glob 列表（如 `["scripts/restore.sh",
     "scripts/*"]`），为空表示不限制并在报告注明。
2. 审查流程改为按任务 acceptance_profile 查档位执行：变更路径校验用
   allowed_paths；固定测试执行 test_cmd（shell 执行、捕获退出码与输出尾部
   进审查证据）。档位缺失或字段缺失时审查直接 fail 并注明原因（不得回退到
   硬编码 counter 行为——counter-v1 档位改为显式配置，行为等价迁移）。
3. 现有 counter-v1 行为迁移为配置且不回归：迁移后跑一次 counter 型任务
   （可用小改动探针）确认审查门结果与迁移前一致。
4. 预置新档位 `service-v1`：test_cmd 为 `cargo fmt --check && cargo clippy
   -- -D warnings && cargo test`（在仓库根执行），allowed_paths 限
   scripts/**、evidence CLI 相关源文件；供 RT-RESTORE-1 / RT-EVIDENCE-1
   两个后续真实任务使用。

## 验收档（acceptance）

- cargo fmt/clippy 干净；store/domain/service/runtime 测试全绿（基线只增不减）。
- 提供迁移等价性证据：counter-v1 迁移前后同一候选的审查判定一致（附运行 ID）。
- 新档位用代码审查方式验收（配置文件 diff + 单元测试覆盖档位解析与
  allowed_paths 匹配逻辑）。
- 提交 diff 与报告符合现有 evidence 规范。

## 约束

- 不改动数据库 schema 中 acceptance_profile 字段语义（仍存档位名）。
- 不在本任务中修改 restore.sh 或 evidence CLI（它们是本档位的首批用户）。
- 允许改动：crates/runtime、crates/domain、crates/service（如需透传档位
  信息）、configs/、scripts/acceptance/（新建脚本目录）、docs/。
